use tucana::shared::{FlowSetting, ValidationFlow, value::Kind};

fn extract_field(settings: &[FlowSetting], def_key: &str, field_name: &str) -> Option<String> {
    settings.iter().find_map(|setting| {
        if setting.flow_setting_id != def_key {
            return None;
        }

        let obj = setting.object.as_ref()?;
        obj.fields.iter().find_map(|(k, v)| {
            if k == field_name {
                if let Some(Kind::StringValue(s)) = &v.kind {
                    return Some(s.clone());
                }
            }
            None
        })
    })
}

/// Every flow identifier needs to start with its
/// flow_id::project_id::flow_identifier::protocol_specific_fields
pub fn get_flow_identifier(flow: &ValidationFlow) -> String {
    match flow.r#type.as_str() {
        "REST" => {
            let method = extract_field(&flow.settings, "HTTP_METHOD", "method");
            let host = extract_field(&flow.settings, "HTTP_HOST", "host");

            let (method, host) = match (method, host) {
                (Some(m), Some(h)) => (m, h),
                missing => {
                    log::error!("Missing settings: {:?}", missing);
                    return format!("{}", flow.flow_id);
                }
            };

            format!(
                "{}::{}::REST::{}::{}",
                flow.flow_id, flow.project_id, host, method
            )
        }
        _ => format!("{}", flow.flow_id),
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use tucana::shared::{FlowSetting, Struct, ValidationFlow as Flow};

    use super::get_flow_identifier;

    fn get_string_value(value: &str) -> tucana::shared::Value {
        tucana::shared::Value {
            kind: Some(tucana::shared::value::Kind::StringValue(String::from(
                value,
            ))),
        }
    }

    #[test]
    fn test_incorrect_flow_type_id() {
        let unkown = Flow {
            starting_node: None,
            flow_id: 1,
            project_id: 1,
            r#type: "UNKOWN_FLOW_TYPE_IDENTIFIER".to_string(),
            data_types: vec![],
            input_type_identifier: None,
            return_type_identifier: None,
            settings: vec![],
        };

        assert_eq!(get_flow_identifier(&unkown), String::from("1"))
    }

    #[test]
    fn test_rest_flow_type_id_is_correct() {
        let rest = Flow {
            starting_node: None,
            flow_id: 1,
            project_id: 1,
            r#type: "REST".to_string(),
            data_types: vec![],
            input_type_identifier: None,
            return_type_identifier: None,
            settings: vec![
                FlowSetting {
                    database_id: 1424525,
                    flow_setting_id: String::from("HTTP_HOST"),
                    object: Some(Struct {
                        fields: {
                            let mut map = HashMap::new();
                            map.insert(String::from("host"), get_string_value("abc.code0.tech"));
                            map
                        },
                    }),
                },
                FlowSetting {
                    database_id: 14245252352,
                    flow_setting_id: String::from("HTTP_METHOD"),
                    object: Some(Struct {
                        fields: {
                            let mut map = HashMap::new();
                            map.insert(String::from("method"), get_string_value("GET"));
                            map
                        },
                    }),
                },
            ],
        };

        let id = get_flow_identifier(&rest);
        assert_eq!(id, String::from("1::1::REST::abc.code0.tech::GET"))
    }

    #[test]
    fn test_rest_flow_type_id_with_missing_settings_fails() {
        let rest = Flow {
            starting_node: None,
            flow_id: 1,
            project_id: 1,
            r#type: "REST".to_string(),
            data_types: vec![],
            input_type_identifier: None,
            return_type_identifier: None,
            settings: vec![],
        };

        assert_eq!(get_flow_identifier(&rest), String::from("1"))
    }
}
