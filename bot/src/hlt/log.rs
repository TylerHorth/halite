use std::fs::File;
use std::io::Write;
use std::process::exit;
use std::sync::Mutex;
use hlt::position::Position;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt;
use std::io::BufWriter;

lazy_static! {
    static ref LOG: Mutex<Log> = Mutex::new(Log::new());
}

pub struct Message {
    turn: usize,
    pos: Position,
    msg: Option<String>,
    col: Option<String>
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{{ \"t\": {}, \"x\": {}, \"y\": {}", self.turn, self.pos.x, self.pos.y)?;

        if let Some(ref message) = self.msg {
            write!(f, ", \"msg\": \"{}\"", message)?;
        }

        if let Some(ref color) = self.col {
            write!(f, ", \"color\": \"{}\"", color)?;
        }

        write!(f, " }},")
    }
}

pub struct Log {
    turn: usize,
    messages: Vec<Message>,
    writer: Option<BufWriter<File>>
}

impl Log {
    pub fn new() -> Log {
        let ret = Log {
            turn: 0,
            messages: Vec::new(),
            writer: None 
        };
        ret
    }

    pub fn open(bot_id: usize) {
        let mut log = LOG.lock().unwrap();

        if log.writer.is_some() {
            drop(log);
            Log::panic(format!("Error: log: tried to open({}) but we have already opened before.", bot_id));
        }

        let filename = format!("bot-{}.log", bot_id);
        let file = File::create(&filename).expect(&format!("Couldn't open file {} for logging!", &filename));
        let mut writer = BufWriter::new(file);

        writeln!(writer, "[").unwrap();

        log.writer = Some(writer);

    }

    pub fn turn(turn_num: usize) {
        let mut log = LOG.lock().unwrap();
        log.turn = turn_num;

        let messages: Vec<_> = log.messages.drain(..).collect();

        let writer = log.writer.as_mut().expect("Must open file before logging.");

        for message in messages {
            writeln!(writer, "{}", message).unwrap();
        }

        writer.flush().unwrap();
    }

    pub fn log(pos: Position, message: impl Into<String>, color: impl Into<String>) {
        let mut log = LOG.lock().unwrap();

        let turn = log.turn;
        let msg = Some(message.into());
        let col = Some(color.into());

        log.messages.push(Message { turn, pos, msg, col });
    }

    pub fn msg(pos: Position, message: impl Into<String>) {
        let mut log = LOG.lock().unwrap();

        let turn = log.turn;
        let msg = Some(message.into());
        let col = None;

        log.messages.push(Message { turn, pos, msg, col });
    }

    pub fn color(pos: Position, color: impl Into<String>) {
        let mut log = LOG.lock().unwrap();

        let turn = log.turn;
        let msg = None;
        let col = Some(color.into());

        log.messages.push(Message { turn, pos, msg, col });
    }

    pub fn flush() {
        let log = LOG.lock().unwrap();
        let turn_num = log.turn;
        drop(log);

        Log::turn(turn_num);
    }

    pub fn flash(message: impl Into<String>, color: impl Into<String>, level: i32) {
        let log = LOG.lock().unwrap();
        let turn_num = log.turn;
        drop(log);

        let zero_pos = Position { x: 0, y: level };
        Log::log(zero_pos, message, color);
        Log::turn(turn_num);
    }

    pub fn info(message: impl Into<String>) {
        Log::flash(message, "#9bc6ff", 0);
    }

    pub fn warn(message: impl Into<String>) {
        Log::flash(message, "#ffa500", 1);
    }

    pub fn error(message: impl Into<String>) {
        Log::flash(message, "#FF0000", 2);
    }

    pub fn panic(message: impl Into<String>) -> ! {
        Log::error(message);

        exit(1)
    }
}
