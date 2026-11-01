//! Background job that drains the runtime usage stream Taurus group-commits
//! per-execution usages into, and forwards them to the Sagittarius gateway on
//! a configurable schedule (`runtime_usage.forward_interval_secs`).
//!
//! Usages are read through a durable pull consumer on a work-queue stream,
//! so several Aquila replicas share the work without forwarding a usage
//! twice. Each NATS message (one Taurus batch) is only acknowledged -- and
//! thereby removed from the stream -- after Sagittarius accepted the request
//! containing it; a rejected request naks its messages so they're retried on
//! the next tick instead of being lost.

use std::time::Duration;

use async_nats::jetstream::{
    self, AckKind,
    consumer::{AckPolicy, PullConsumer, pull},
    stream::RetentionPolicy,
};
use futures::StreamExt;
use prost::Message;
use tucana::sagittarius_gateway::RuntimeUsageRequest;

use super::runtime_usage_service_client_impl::SagittariusRuntimeUsageServiceClient;
use crate::telemetry::errors;

/// Durable consumer name shared by every Aquila replica.
const CONSUMER_NAME: &str = "aquila_runtime_usage";

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub struct RuntimeUsageForwarderSettings {
    pub stream: String,
    pub forward_interval: Duration,
    pub max_batches_per_request: usize,
}

/// Spawns the periodic forwarder. Runs until the process exits; the caller
/// supervises it via the returned `JoinHandle`.
pub fn spawn(
    jetstream: jetstream::Context,
    mut client: SagittariusRuntimeUsageServiceClient,
    settings: RuntimeUsageForwarderSettings,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(settings.forward_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut consumer: Option<PullConsumer> = None;

        log::info!(
            "Runtime usage forwarder started stream={} interval_secs={} max_batches_per_request={}",
            settings.stream,
            settings.forward_interval.as_secs(),
            settings.max_batches_per_request
        );

        loop {
            interval.tick().await;

            if consumer.is_none() {
                match ensure_consumer(&jetstream, &settings.stream).await {
                    Ok(created) => consumer = Some(created),
                    Err(err) => {
                        errors::record(
                            "messaging",
                            "runtime_usage.consumer.ensure",
                            err.as_ref(),
                            format!("stream={}", settings.stream),
                        );
                        continue;
                    }
                }
            }

            let Some(active_consumer) = consumer.as_ref() else {
                continue;
            };

            if let Err(err) = forward_pending(active_consumer, &mut client, &settings).await {
                errors::record(
                    "messaging",
                    "runtime_usage.forward",
                    err.as_ref(),
                    format!("stream={}", settings.stream),
                );
                // Re-resolve the stream and consumer on the next tick in case
                // they were deleted or NATS was restarted without persistence.
                consumer = None;
            }
        }
    })
}

/// Creates the work-queue stream and the durable consumer if they don't
/// exist yet. Taurus creates the same stream on its side, so the stream
/// configuration must stay identical in both services.
async fn ensure_consumer(
    jetstream: &jetstream::Context,
    stream_name: &str,
) -> Result<PullConsumer, BoxError> {
    let stream = jetstream
        .get_or_create_stream(jetstream::stream::Config {
            name: stream_name.to_string(),
            subjects: vec![stream_name.to_string()],
            retention: RetentionPolicy::WorkQueue,
            ..Default::default()
        })
        .await?;

    let consumer = stream
        .get_or_create_consumer(
            CONSUMER_NAME,
            pull::Config {
                durable_name: Some(CONSUMER_NAME.to_string()),
                ack_policy: AckPolicy::Explicit,
                ..Default::default()
            },
        )
        .await?;

    Ok(consumer)
}

/// Forwards everything currently in the stream, one Sagittarius request per
/// `max_batches_per_request` NATS messages, stopping early when Sagittarius
/// rejects a request.
async fn forward_pending(
    consumer: &PullConsumer,
    client: &mut SagittariusRuntimeUsageServiceClient,
    settings: &RuntimeUsageForwarderSettings,
) -> Result<(), BoxError> {
    let max_batches = settings.max_batches_per_request.max(1);

    loop {
        let mut batch = consumer
            .fetch()
            .max_messages(max_batches)
            .messages()
            .await?;
        let mut fetched = 0;
        let mut messages = Vec::new();
        let mut runtime_usage = Vec::new();

        while let Some(message) = batch.next().await {
            let message = message?;
            fetched += 1;

            match RuntimeUsageRequest::decode(message.payload.as_ref()) {
                Ok(request) => {
                    runtime_usage.extend(request.runtime_usage);
                    messages.push(message);
                }
                Err(err) => {
                    // A message that can't be decoded never will be; drop it
                    // instead of redelivering it forever.
                    errors::record(
                        "serialization",
                        "runtime_usage.decode",
                        &err,
                        format!("payload_bytes={}", message.payload.len()),
                    );
                    if let Err(err) = message.ack_with(AckKind::Term).await {
                        log::warn!(
                            "Failed to terminate undecodable runtime usage message error={err}"
                        );
                    }
                }
            }
        }

        if fetched == 0 {
            return Ok(());
        }

        if !runtime_usage.is_empty() && !client.update_runtime_usage(runtime_usage).await {
            for message in &messages {
                if let Err(err) = message
                    .ack_with(AckKind::Nak(Some(settings.forward_interval)))
                    .await
                {
                    log::warn!("Failed to nak runtime usage message error={err}");
                }
            }
            return Ok(());
        }

        for message in &messages {
            if let Err(err) = message.ack().await {
                // The message will be redelivered after the ack wait and
                // forwarded again; Sagittarius has to tolerate that rare
                // duplicate rather than Aquila losing usage.
                log::warn!("Failed to acknowledge forwarded runtime usage message error={err}");
            }
        }

        if fetched < max_batches {
            return Ok(());
        }
    }
}
