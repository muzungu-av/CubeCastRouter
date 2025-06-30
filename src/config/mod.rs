use std::fs;

pub(crate) struct RoomConfig {
    pub room: String,
    pub teacher: String,
    pub authorised_students: Vec<String>,
    pub sign_key: String,
}

impl RoomConfig {
    /// Читает файл по указанному пути и парсит три строки вида:
    /// room = room_568491
    /// teacher = user_1
    /// authorised_students = user_2, user_3, user_4
    pub fn load_from_file(path: &str) -> Self {
        // Вариант: мы сразу же паникуем, если файл не найден или некорректный формат
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read config file `{}`: {:?}", path, e));
        let mut room = None;
        let mut teacher = None;
        let mut authorised_students = None;
        let mut sign_key = None;

        for (lineno, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Разбиваем по первому '='
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim();
                let val = line[pos + 1..].trim();
                match key {
                    "room" => {
                        room = Some(val.to_string());
                    }
                    "teacher" => {
                        teacher = Some(val.to_string());
                    }
                    "sign_key" => {
                        sign_key = Some(val.to_string());
                    }
                    "authorised_students" => {
                        // Разделяем по запятым, удаляем пробелы
                        let vec = val
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>();
                        authorised_students = Some(vec);
                    }
                    other => {
                        panic!(
                            "Unknown key `{}` in config file at line {}",
                            other,
                            lineno + 1
                        );
                    }
                }
            } else {
                panic!(
                    "Bad line in config (no '=') at {}: `{}`",
                    lineno + 1,
                    raw_line
                );
            }
        }

        // Убедимся, что все поля заданы:
        let room = room.unwrap_or_else(|| panic!("`room` is missing in config"));
        let teacher = teacher.unwrap_or_else(|| panic!("`teacher` is missing in config"));
        let authorised_students = authorised_students.unwrap_or_else(Vec::new);
        let sign_key = sign_key.unwrap_or_else(|| panic!("`sign_key` is missing in config"));

        RoomConfig {
            room,
            teacher,
            authorised_students,
            sign_key,
        }
    }
}
