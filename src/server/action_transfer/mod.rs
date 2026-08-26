//! gRPC server for the bidirectional `ActionTransfer` stream: authenticates
//! an action's logon, then bridges execution requests/results and config
//! updates between NATS and the connected action for the lifetime of the
//! stream.
//!
//! The module is split by responsibility:
//! - [`logon`] validates the initial logon message and registers the action.
//! - [`nats_bridge`] moves execution requests/results between NATS and gRPC.
//! - [`pending_replies`] tracks which NATS reply subject an execution result belongs to.
//! - [`flow_execution_registry`] tracks which action stream an action-triggered
//!   flow execution's result belongs to.

mod flow_execution_registry;
mod logon;
mod nats_bridge;
mod pending_replies;
mod shard_registry;

pub use flow_execution_registry::ActionFlowExecutionRegistry;
pub use shard_registry::{ActionShardRegistry, ShardAssignment};

use std::{future::Future, pin::Pin, sync::Arc};

use futures::StreamExt;
use futures_core::Stream;
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use tracing::Instrument;
use tucana::aquila::{
    ActionTransferRequest, ActionTransferResponse,
    action_transfer_service_server::ActionTransferService,
};

use crate::{
    configuration::service::ServiceConfiguration, flow::FlowChange,
    sagittarius::module_service_client_impl::SagittariusModuleServiceClient, telemetry::metrics,
};

use logon::{extract_token, handle_logon};
use nats_bridge::{
    handle_flow_execution, handle_result, handle_sub_flow_execution, send_stream_error,
};
use pending_replies::PendingReplyStore;

/// Every dependency an action's connection needs, bundled into one
/// `Clone`-able value instead of threaded individually through every layer
/// (`transfer` -> `handle_logon` -> the NATS bridge handlers). Adding a new
/// shared dependency means adding a field here, not a parameter everywhere
/// it's needed.
#[derive(Clone)]
pub(super) struct ActionTransferContext {
    pub(super) client: async_nats::Client,
    pub(super) kv: async_nats::jetstream::kv::Store,
    /// Static, pre-provisioned action tokens/configuration loaded at startup.
    /// Read-only after startup, so it's shared directly rather than behind a lock.
    pub(super) actions: ServiceConfiguration,
    /// Present only in dynamic mode, where module updates must be relayed to Sagittarius.
    pub(super) module_service: Option<Arc<Mutex<SagittariusModuleServiceClient>>>,
    /// Broadcasts module configuration updates to every connected action's config forwarder.
    pub(super) action_config_tx:
        tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    /// Broadcasts flow store changes to every connected action's flow forwarder.
    pub(super) action_flow_tx: tokio::sync::broadcast::Sender<FlowChange>,
    /// Correlates action-triggered flow executions with the action stream to
    /// deliver their result to, once a runtime reports it.
    pub(super) flow_execution_registry: ActionFlowExecutionRegistry,
    /// Tracks which shard index each `Split`-scaled action connection owns,
    /// so concurrent connections for one identifier partition project-scoped
    /// updates instead of each receiving everything. See [`shard_registry`].
    pub(super) shard_registry: ActionShardRegistry,
    /// Whether Aquila is running in static mode, which changes how config updates are sourced.
    pub(super) is_static: bool,
    /// Per-connection limit for post-logon message handlers. Parsing stays on
    /// the stream task; handler work runs in this bounded task set.
    pub(super) concurrency_limit: usize,
}

/// Owns every post-logon handler spawned for one action stream.
///
/// Permits are acquired inside the spawned tasks so a slow handler never
/// prevents the stream task from parsing later messages or noticing EOF. The
/// join set gives the connection a single place to reap completed work and to
/// cancel everything that is still running when the stream closes.
struct PostLogonTaskSet {
    semaphore: Arc<Semaphore>,
    tasks: JoinSet<()>,
}

impl PostLogonTaskSet {
    fn new(concurrency_limit: usize) -> Self {
        assert!(
            concurrency_limit > 0,
            "grpc.action_transfer_concurrency_limit must be at least 1"
        );

        Self {
            semaphore: Arc::new(Semaphore::new(concurrency_limit)),
            tasks: JoinSet::new(),
        }
    }

    fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.reap_finished();

        let semaphore = self.semaphore.clone();
        self.tasks.spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return;
            };
            future.await;
        });
    }

    fn reap_finished(&mut self) {
        while let Some(result) = self.tasks.try_join_next() {
            if let Err(error) = result {
                log::warn!("Action transfer message handler failed: {error}");
            }
        }
    }

    async fn shutdown(&mut self) {
        self.tasks.abort_all();
        while let Some(result) = self.tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                log::warn!("Action transfer message handler failed during shutdown: {error}");
            }
        }
    }
}

/// Implements the `ActionTransfer` gRPC service that a connected action
/// speaks to for the lifetime of its bidirectional stream.
///
/// One instance is shared across all connected actions; per-connection state
/// (pending replies, logon status, ...) lives inside the `transfer` task
/// spawned for each stream instead of on `self`.
pub struct AquilaActionTransferServiceServer {
    context: ActionTransferContext,
}

impl AquilaActionTransferServiceServer {
    pub(super) fn new(context: ActionTransferContext) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl ActionTransferService for AquilaActionTransferServiceServer {
    type TransferStream =
        Pin<Box<dyn Stream<Item = Result<ActionTransferResponse, tonic::Status>> + Send + 'static>>;

    /// Opens the bidirectional stream an action uses to log on and then
    /// exchange events, execution requests, and results with Aquila.
    ///
    /// The gRPC protocol for this stream is a strict handshake: the first
    /// message must be a [`Logon`](tucana::aquila::action_transfer_request::Data::Logon),
    /// every message after that must not be, and everything is driven from a
    /// single spawned task so the stream can be read and written to concurrently.
    #[tracing::instrument(
        name = "aquila.action.transfer",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "ActionTransferService", rpc.method = "Transfer")
    )]
    async fn transfer(
        &self,
        request: tonic::Request<tonic::Streaming<ActionTransferRequest>>,
    ) -> std::result::Result<tonic::Response<Self::TransferStream>, tonic::Status> {
        let token = extract_token(&request)?;
        log::debug!("Action transfer stream opened");

        let mut first_request = true;
        let mut action_props: Option<tucana::aquila::ActionLogon> = None;
        let mut stream = request.into_inner();

        let context = self.context.clone();
        let pending_replies = PendingReplyStore::new();

        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<ActionTransferResponse, tonic::Status>>(32);

        let stream_span = tracing::info_span!(
            "aquila.action.stream",
            action.identifier = tracing::field::Empty
        );
        tokio::spawn(async move {
            let mut post_logon_tasks = PostLogonTaskSet::new(context.concurrency_limit);
            let mut cfg_forwarder_started = false;
            let mut flow_forwarder_started = false;
            let mut connected_at = None;
            let mut connected_identifier = None;
            let mut connected_shard: Option<ShardAssignment> = None;
            log::debug!("Action transfer stream started");

            while let Some(next) = stream.next().await {
                let transfer_request = match next {
                    Ok(tr) => tr,
                    Err(status) => {
                        log::warn!("Action transfer input stream failed status={:?}", status);
                        break;
                    }
                };

                let data = match transfer_request.data {
                    Some(d) => d,
                    None => {
                        log::warn!("Received empty action transfer request");
                        continue;
                    }
                };

                if first_request {
                    first_request = false;

                    match data {
                        tucana::aquila::action_transfer_request::Data::Logon(action_logon) => {
                            let identifier = match action_logon.module {
                                Some(ref m) => m.identifier.clone(),
                                None => {
                                    log::warn!("Rejected action logon reason=missing_module");
                                    send_stream_error(
                                        &tx,
                                        Status::aborted("Please provide a module configuration."),
                                    )
                                    .await;
                                    break;
                                }
                            };

                            log::debug!("Received logon for action {}", identifier);

                            let (accepted, shard) = match handle_logon(
                                &token,
                                action_logon,
                                context.clone(),
                                tx.clone(),
                                pending_replies.clone(),
                                &mut cfg_forwarder_started,
                                &mut flow_forwarder_started,
                            )
                            .await
                            {
                                Ok(v) => v,
                                Err(status) => {
                                    log::warn!(
                                        "Action logon failed identifier={} code={:?} message={}",
                                        identifier,
                                        status.code(),
                                        status.message()
                                    );
                                    send_stream_error(&tx, status).await;
                                    break;
                                }
                            };

                            action_props = Some(accepted);
                            tracing::Span::current()
                                .record("action.identifier", identifier.as_str());
                            metrics::action_connection(&identifier, "accepted");
                            metrics::action_active(&identifier, 1);
                            connected_at = Some(std::time::Instant::now());
                            connected_shard = shard;
                            if let Some(shard) = shard {
                                log_unclaimed_shards(&context, &identifier, shard.replicas).await;
                            }
                            connected_identifier = Some(identifier);
                        }
                        _ => {
                            log::error!("Action stream protocol violation expected=logon");
                            send_stream_error(
                                &tx,
                                Status::failed_precondition("first action stream message must be logon"),
                            )
                            .await;
                            break;
                        }
                    }

                    continue;
                }

                let props = match action_props.clone() {
                    Some(p) => p,
                    None => {
                        log::error!("Missing action properties after logon");
                        break;
                    }
                };

                let identifier = match props.module {
                    Some(ref m) => m.identifier.clone(),
                    None => {
                        log::error!("Logon state missing module");
                        break;
                    }
                };

                // Static mode has no Sagittarius to push config updates from, so
                // re-broadcast the action's configured values on every message it
                // sends instead of relying on a one-shot delivery at logon time.
                if context.is_static {
                    let configs = context.actions.get_action_configuration(&token, &identifier);
                    for conf in configs {
                        if let Err(err) = context.action_config_tx.send(conf) {
                            log::warn!("No action configuration receivers available: {:?}", err);
                        }
                    }
                };

                match data {
                    tucana::aquila::action_transfer_request::Data::Logon(_) => {
                        log::warn!(
                            "Action stream protocol violation identifier={} reason=duplicate_logon",
                            identifier
                        );
                        send_stream_error(
                            &tx,
                            Status::failed_precondition("action stream logon was already accepted"),
                        )
                        .await;
                        break;
                    }
                    tucana::aquila::action_transfer_request::Data::Result(execution_result) => {
                        log::debug!(
                            "Received execution result execution_id={} action={}",
                            execution_result.execution_identifier,
                            identifier
                        );

                        let client = context.client.clone();
                        let pending_replies = pending_replies.clone();
                        post_logon_tasks.spawn(async move {
                            handle_result(
                                &identifier,
                                execution_result,
                                client,
                                pending_replies,
                            )
                            .await;
                        });
                    }
                    tucana::aquila::action_transfer_request::Data::SubFlowExecution(request) => {
                        log::debug!(
                            "Received sub flow execution request action={} execution_id={}",
                            identifier,
                            request.execution_identifier
                        );

                        let client = context.client.clone();
                        let tx = tx.clone();
                        post_logon_tasks.spawn(async move {
                            handle_sub_flow_execution(&identifier, request, client, tx).await;
                        });
                    }
                    tucana::aquila::action_transfer_request::Data::FlowExecution(request) => {
                        log::debug!(
                            "Received flow execution request action={} execution_id={} flow_id={}",
                            identifier,
                            request.execution_identifier,
                            request.flow_id
                        );

                        let kv = context.kv.clone();
                        let client = context.client.clone();
                        let registry = context.flow_execution_registry.clone();
                        let tx = tx.clone();
                        post_logon_tasks.spawn(async move {
                            handle_flow_execution(
                                &identifier,
                                request,
                                kv,
                                client,
                                registry,
                                tx,
                            )
                            .await;
                        });
                    }
                }
            }

            // Handler futures may be blocked on NATS or waiting for a permit.
            // Once the stream is gone none can produce a useful response, so
            // cancel and join them before releasing per-connection state.
            post_logon_tasks.shutdown().await;

            if let Some(identifier) = connected_identifier {
                metrics::action_active(&identifier, -1);
                metrics::action_connection(&identifier, "closed");
                if let Some(connected_at) = connected_at {
                    metrics::action_connection_duration(
                        &identifier,
                        connected_at.elapsed().as_secs_f64(),
                    );
                }
                if let Some(shard) = connected_shard {
                    context.shard_registry.release(&identifier, shard.index).await;
                    log_unclaimed_shards(&context, &identifier, shard.replicas).await;
                }
            }
            log::debug!("Action transfer stream ended");
        }
        .instrument(stream_span));

        Ok(tonic::Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Logs every shard index in `0..replicas` for `identifier` with no current
/// claimant, so an under-scaled or misconfigured deployment (fewer `Split`
/// connections than the configured `replicas`) is a visible operational
/// signal instead of a silent gap in project-scoped updates.
async fn log_unclaimed_shards(context: &ActionTransferContext, identifier: &str, replicas: u32) {
    let unclaimed = context.shard_registry.unclaimed(identifier, replicas).await;
    if !unclaimed.is_empty() {
        log::warn!(
            "Action has unclaimed shards identifier={} replicas={} unclaimed={:?}",
            identifier,
            replicas,
            unclaimed
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::{Notify, Semaphore, mpsc};

    use super::PostLogonTaskSet;

    #[tokio::test]
    async fn parallel_flow_starts_are_not_serialized() {
        let mut tasks = PostLogonTaskSet::new(2);
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let release = Arc::new(Semaphore::new(0));

        for execution_id in ["flow-1", "flow-2"] {
            let started_tx = started_tx.clone();
            let release = release.clone();
            tasks.spawn(async move {
                started_tx.send(execution_id).unwrap();
                let _permit = release.acquire().await.unwrap();
            });
        }

        let first = tokio::time::timeout(std::time::Duration::from_secs(1), started_rx.recv())
            .await
            .expect("first flow should start")
            .unwrap();
        let second = tokio::time::timeout(std::time::Duration::from_secs(1), started_rx.recv())
            .await
            .expect("second flow should start while the first is running")
            .unwrap();

        assert_ne!(first, second);
        release.add_permits(2);
        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn slow_subflow_does_not_block_an_unrelated_flow() {
        let mut tasks = PostLogonTaskSet::new(2);
        let slow_subflow = Arc::new(Notify::new());
        let (subflow_started_tx, subflow_started_rx) = tokio::sync::oneshot::channel();
        let (flow_started_tx, flow_started_rx) = tokio::sync::oneshot::channel();

        let slow_subflow_task = slow_subflow.clone();
        tasks.spawn(async move {
            subflow_started_tx.send(()).unwrap();
            slow_subflow_task.notified().await;
        });
        subflow_started_rx.await.unwrap();

        tasks.spawn(async move {
            flow_started_tx.send(()).unwrap();
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), flow_started_rx)
            .await
            .expect("flow should start while the subflow is still waiting")
            .unwrap();

        slow_subflow.notify_one();
        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn configured_concurrency_limit_is_never_exceeded() {
        const LIMIT: usize = 3;
        const TASK_COUNT: usize = 12;

        let mut tasks = PostLogonTaskSet::new(LIMIT);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Semaphore::new(0));
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let (finished_tx, mut finished_rx) = mpsc::unbounded_channel();

        for _ in 0..TASK_COUNT {
            let active = active.clone();
            let maximum = maximum.clone();
            let release = release.clone();
            let started_tx = started_tx.clone();
            let finished_tx = finished_tx.clone();
            tasks.spawn(async move {
                let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now_active, Ordering::SeqCst);
                started_tx.send(()).unwrap();

                let _release = release.acquire().await.unwrap();
                active.fetch_sub(1, Ordering::SeqCst);
                finished_tx.send(()).unwrap();
            });
        }

        for _ in 0..LIMIT {
            tokio::time::timeout(std::time::Duration::from_secs(1), started_rx.recv())
                .await
                .expect("task up to the limit should start")
                .unwrap();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), started_rx.recv())
                .await
                .is_err(),
            "a task beyond the limit started before a permit was released"
        );

        release.add_permits(TASK_COUNT);
        for _ in 0..TASK_COUNT {
            tokio::time::timeout(std::time::Duration::from_secs(1), finished_rx.recv())
                .await
                .expect("all tasks should finish")
                .unwrap();
        }

        assert_eq!(maximum.load(Ordering::SeqCst), LIMIT);
        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn stream_shutdown_cancels_outstanding_tasks() {
        struct NotifyOnDrop(Option<tokio::sync::oneshot::Sender<()>>);

        impl Drop for NotifyOnDrop {
            fn drop(&mut self) {
                if let Some(tx) = self.0.take() {
                    let _ = tx.send(());
                }
            }
        }

        let mut tasks = PostLogonTaskSet::new(1);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();

        tasks.spawn(async move {
            let _drop_notification = NotifyOnDrop(Some(dropped_tx));
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.await.unwrap();

        tasks.shutdown().await;

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
            .await
            .expect("shutdown should drop the outstanding handler future")
            .unwrap();
        assert!(tasks.tasks.is_empty());
    }
}
