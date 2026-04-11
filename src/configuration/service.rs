use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::{fs::File, io::Read};
use tucana::shared::ActionConfigurations;

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
    config: Vec<ActionConfigurations>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServiceConfiguration {
    actions: Vec<ActionServiceConfiguration>,
}

impl ServiceConfiguration {
    pub fn has_action(&self, token: &String, action_identifier: &String) -> bool {
        match self
            .actions
            .iter()
            .find(|x| &x.token == token && &x.service_name == action_identifier)
        {
            Some(_) => true,
            None => false,
        }
    }

    pub fn get_action_configuration(
        &self,
        action_identifier: &String,
    ) -> Vec<ActionConfigurations> {
        match self
            .actions
            .iter()
            .find(|x| &x.service_name == action_identifier)
        {
            Some(a) => a.config.clone(),
            None => vec![],
        }
    }

    pub fn from_path(path: &String) -> Self {
        let mut data = String::new();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "Couldn't open service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ActionConfiguration { actions: vec![] };
            }
        };

        match file.read_to_string(&mut data) {
            Ok(_) => {
                log::debug!("Successfully loaded action configuration file");
            }
            Err(error) => {
                log::warn!(
                    "Couldn't read service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ActionConfiguration { actions: vec![] };
            }
        }

        match from_str::<ServiceConfiguration>(&data) {
            Ok(conf) => return conf,
            Err(error) => {
                log::warn!(
                    "Couldn't parse service configuration file, Reason: {}. Starting with empty service configuration",
                    error
                );
                return ActionConfiguration { actions: vec![] };
            }
        };
    }
}
