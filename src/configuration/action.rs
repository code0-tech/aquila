use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::{fs::File, io::Read};

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionServiceConfiguration {
    token: String,
    service_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionConfiguration {
    actions: Vec<ActionServiceConfiguration>,
}

impl ActionConfiguration {
    pub fn has_action(self, token: String, action_identifier: &String) -> bool {
        match self
            .actions
            .into_iter()
            .find(|x| x.token == token && x.service_name == *action_identifier)
        {
            Some(_) => true,
            None => false,
        }
    }

    pub fn from_path(path: &String) -> Self {
        let mut data = String::new();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                panic!("There was a problem opening the file: {:?}", error);
            }
        };

        match file.read_to_string(&mut data) {
            Ok(_) => {
                log::debug!("Successfully loaded action configuration file");
            }
            Err(error) => {
                panic!("There was a problem reading the file: {:?}", error);
            }
        }

        match from_str::<ActionConfiguration>(&data) {
            Ok(conf) => return conf,
            Err(error) => {
                panic!(
                    "There was a problem deserializing the json file: {:?}",
                    error
                );
            }
        };
    }
}
