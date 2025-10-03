#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

static mut LOG_LEVEL: LogLevel = LogLevel::Info;

pub unsafe fn set_log_level(level: LogLevel) {
    LOG_LEVEL = level;
}

pub unsafe fn get_log_level() -> LogLevel {
    LOG_LEVEL
}

#[macro_export]
macro_rules! __log_internal {
    ($level:expr, $level_str:expr, $($arg:tt)*) => {
        {
            let current_level = unsafe { log::get_log_level() };
            if $level as u8 >= current_level as u8 {
                println!("[{}] {}", $level_str, format_args!($($arg)*));
            }
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::__log_internal!(log::LogLevel::Debug, "DEBUG", $($arg)*)
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::__log_internal!(log::LogLevel::Info, "INFO", $($arg)*)
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::__log_internal!(log::LogLevel::Warn, "WARN", $($arg)*)
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::__log_internal!(log::LogLevel::Error, "ERROR", $($arg)*)
    };
}

pub fn init(level: LogLevel) {
    unsafe {
        set_log_level(level);
    }
}

pub fn init_debug() {
    init(LogLevel::Debug);
}

pub fn init_info() {
    init(LogLevel::Info);
}

pub fn init_warn() {
    init(LogLevel::Warn);
}

pub fn init_error() {
    init(LogLevel::Error);
}