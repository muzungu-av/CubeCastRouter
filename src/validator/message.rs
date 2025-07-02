use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sender {
    pub id: String,
    #[serde(rename = "type")]
    pub sender_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub scope: String,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IncomingMessage {
    #[serde(rename = "room_id")]
    pub room_id: String,
    pub sender: Sender,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(rename = "command")]
    pub msg_command: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

pub trait Validate {
    fn validate(&self) -> Result<(), String>;
}

impl Validate for IncomingMessage {
    fn validate(&self) -> Result<(), String> {
        if self.room_id.trim().is_empty() {
            return Err("Поле room_id не должно быть пустым".into());
        }
        /*
        "учитель" - проводит занятие
        "ученик"  - участник занятий
        "наблюдатель" - наблюает втайне (родитель / невидимый )
        "ADMIN"       - тоже что наблюдатель (невидимый) и учитель (все права)
        "heartbeat"   - для PING-сообщений сервера
        */
        match self.sender.sender_type.as_str() {
            "учитель" | "ученик" | "наблюдатель" | "ADMIN" | "heartbeat" => {
            }
            other => {
                return Err(format!(
                    "Неверный sender.type: {}. Должно быть 'учитель', 'ученик' или 'наблюдатель'",
                    other
                ))
            }
        }
        if let Some(target) = &self.target {
            match target.scope.as_str() {
                "all" => {}
                "type" => {
                    if target.types.is_empty() {
                        return Err(
                            "Когда target.scope = 'type', поле target.types не должно быть пустым"
                                .into(),
                        );
                    }
                }
                "ids" => {
                    if target.ids.is_empty() {
                        return Err(
                            "Когда target.scope = 'ids', поле target.ids не должно быть пустым"
                                .into(),
                        );
                    }
                }
                other => {
                    return Err(format!(
                        "Неверное target.scope: '{}'. Должно быть 'all', 'type' или 'ids'",
                        other
                    ))
                }
            }
        }
        if self.msg_command.trim().is_empty() {
            return Err("Поле msg_command не должно быть пустым".into());
        }
        Ok(())
    }
}
