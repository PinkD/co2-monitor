use alloc::string::String;
use esp_println::println;
use crate::{debug, log};


pub struct DebugPrinter {
    name: String,
}

impl DebugPrinter {
    pub fn new(name: String) -> Self {
        debug!("enter {}", name);
        DebugPrinter { name }
    }
}

impl Drop for DebugPrinter {
    fn drop(&mut self) {
        debug!("exit {}", self.name);
    }
}

pub fn debug_alloc(s: &str) {
    let stats = esp_alloc::HEAP.stats();
    debug!("{} heap stats: {}", s, stats);
}
