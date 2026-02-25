use crate::configuration::action::ActionConfiguration;
use futures_core::Stream;
use std::pin::Pin;
use tokio_stream::StreamExt;
use tonic::Status;
use tucana::aquila::{
    ActionLogon, TransferRequest, TransferResponse,
    action_transfer_service_server::ActionTransferService,
};

pub struct AquilaActionTransferServiceServer {
    client: async_nats::Client,
    actions: ActionConfiguration,
}

impl AquilaActionTransferServiceServer {
    pub fn new(client: async_nats::Client, actions: ActionConfiguration) -> Self {
        Self { client, actions }
    }
}

#[tonic::async_trait]
impl ActionTransferService for AquilaActionTransferServiceServer {
    type TransferStream =
        Pin<Box<dyn Stream<Item = Result<TransferResponse, tonic::Status>> + Send + 'static>>;

    async fn transfer(
        &self,
        request: tonic::Request<tonic::Streaming<TransferRequest>>,
    ) -> std::result::Result<tonic::Response<Self::TransferStream>, tonic::Status> {
        let mut first_request = true;
        let mut action_props: Option<ActionLogon> = None;
        let token = match request.metadata().get("authorization") {
            Some(ascii) => match ascii.to_str() {
                Ok(tk) => tk.to_string(),
                Err(err) => {
                    log::error!("Cannot read authorization header because: {:?}", err);
                    return Err(Status::internal("cannot read authorization header"));
                }
            },
            None => return Err(Status::unauthenticated("missing authorization token")),
        };

        let mut stream = request.into_inner();
        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                let transfer_request = match next {
                    Ok(tr) => tr,
                    Err(status) => {
                        log::error!("Stream was closed with status code: {:?}", status);
                        break;
                    }
                };

                let data = match transfer_request.data {
                    Some(d) => d,
                    None => {
                        log::error!("Recieved empty request, waiting on next one");
                        continue;
                    }
                };

                if first_request {
                    first_request = false;

                    match data {
                        tucana::aquila::transfer_request::Data::Logon(action_logon) => {
                            log::info!("Action successfull logged on: {:?}", action_logon);
                            action_props = Some(action_logon);
                            todo!("check for authentication!")
                        }
                        _ => {
                            log::error!(
                                "Action tried to logon but was not sending a logon request!"
                            );
                            //return Err(Status::internal("First request needs to be a 'ActionLogonRequest'"));
                        }
                    }
                    continue;
                }

                let props = match action_props {
                    Some(ref p) => p.clone(),
                    None => todo!("return internal error"),
                };

                match data {
                    tucana::aquila::transfer_request::Data::Logon(action_logon) => todo!("Reject because already logged on"),
                    tucana::aquila::transfer_request::Data::Event(event) => todo!("check flows, push into execution queue"),
                    tucana::aquila::transfer_request::Data::Result(execution_result) => todo!("respond into nats with result"),
                }
            }
        });

        /*
                let action_configuration_update_request = request.into_inner();
                match self.actions.clone().has_action(
                    token,
                    &action_configuration_update_request.action_identifier,
                ) {
                    true => {
                        log::debug!(
                            "Action with identifer: {}, connected successfully",
                            action_configuration_update_request.action_identifier
                        );
                    }
                    false => {
                        log::debug!(
                            "Rejected action with identifer: {}, becuase its not registered",
                            action_configuration_update_request.action_identifier
                        );
                        return Err(Status::unauthenticated(""));
                    }
                }

        */

        unimplemented!();
    }
}
