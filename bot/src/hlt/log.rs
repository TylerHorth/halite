use std::fs::File;
use std::io::Write;
use std::process::exit;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use std::collections::HashMap;
use hlt::position::Position;

lazy_static! {
    static ref LOG: Mutex<Log> = Mutex::new(Log::new());
}

pub struct Log {
    turn_num: usize,

    messages: HashMap<Position, (Option<String>, Option<String>)>,

    log_buffer: Option<Vec<String>>,
    file: Option<File>,
}

impl Log {
    pub fn new() -> Log {
        let mut buffer = Vec::new();
        buffer.push("[ ".into());
        let ret = Log {turn_num: 0, messages: HashMap::new(), log_buffer: Some(buffer), file: None };
        ret
    }

    pub fn open(bot_id: usize) {
        let mut log = LOG.lock().unwrap();

        if log.file.is_some() {
            Log::panic_inner(&mut log, &format!("Error: log: tried to open({}) but we have already opened before.", bot_id));
        }

        let filename = format!("bot-{}.log", bot_id);
        let mut file = File::create(&filename).expect(&format!("Couldn't open file {} for logging!", &filename));

        Log::dump_log_buffer(&log.log_buffer, &mut file);

        log.file = Some(file);
        log.log_buffer = None;
    }

    pub fn turn(turn_num: usize) {
        let mut log = LOG.lock().unwrap();

        if log.turn_num != turn_num {
            log.turn_num = turn_num;

            let messages: Vec<_> = log.messages.drain().collect();

            for (pos, (message, color)) in messages {
                let mut record = format!("{{ \"t\": {}, \"x\": {}, \"y\": {}", turn_num, pos.x, pos.y);

                if let Some(message) = message {
                    record.push_str(&format!(", \"msg\": \"{}\"", message));
                }

                if let Some(color) = color {
                    record.push_str(&format!(", \"color\": \"{}\"", color));
                }

                record.push_str(" },\n");

                match &mut log.file {
                    Some(file) => {
                        writeln!(file, "{}", record).unwrap();
                        return;
                    },
                    None => ()
                }

                match &mut log.log_buffer {
                    Some(log_buffer) => {
                        log_buffer.push(record);
                    },
                    None => panic!("Error: both file and log_buffer as missing.")
                }
            }
        }

    }

    pub fn log(pos: Position, message: impl Into<String>, color: impl Into<String>) {
        let mut log = LOG.lock().unwrap();

        let message = message.into();
        let color = color.into();

        log.messages.entry(pos)
            .and_modify(|e| {
                if e.0.is_some() {
                    let m = e.0.take().unwrap();
                    e.0 = Some(m + ":" + &message.clone());
                } else {
                    e.0 = Some(message.clone());
                }

                e.1 = Some(color.clone());
            })
            .or_insert((Some(message.clone()), Some(color.clone())));
    }

    pub fn msg(pos: Position, message: &str) {
        let mut log = LOG.lock().unwrap();

        log.messages.entry(pos)
            .and_modify(|e|  e.0 = e.clone().0.map_or(Some(message.into()), |m| Some(m + ":" + message)))
            .or_insert((Some(message.into()), None));
    }

    pub fn color(pos: Position, color: &str) {
        let mut log = LOG.lock().unwrap();

        log.messages.entry(pos)
            .and_modify(|e| e.1 = Some(color.into()))
            .or_insert((None, Some(color.into())));
    }

    pub fn flush() {
        let mut log = LOG.lock().unwrap();

        match &mut log.file {
            Some(file) => { file.flush().unwrap(); },
            None => (),
        }
    }

    pub fn panic(message: &str) -> ! {
        let mut log = LOG.lock().unwrap();
        Log::panic_inner(&mut log, message)
    }

    fn panic_inner(log: &mut Log, message: &str) -> ! {
        if log.file.is_none() {
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let filename = format!("bot-unknown-{}.log", timestamp.as_secs());
            let file = File::create(&filename).expect(&format!("Couldn't open file {} for logging!", &filename));
            log.file = Some(file);
        }

        let file = match &mut log.file {
            Some(file) => file,
            None => panic!("Error: file should exist!")
        };

        Log::dump_log_buffer(&log.log_buffer, file);

        writeln!(file, "{}", message).unwrap();
        file.flush().unwrap();

        exit(1);
    }

    fn dump_log_buffer(log_buffer: &Option<Vec<String>>, file: &mut File) {
        match log_buffer {
            Some(log_buffer) => {
                for message in log_buffer {
                    writeln!(file, "{}", message).unwrap();
                }
            }
            None => ()
        }
    }
}
