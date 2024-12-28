use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use teloxide::prelude::Message;

const CACHE_SIZE: usize = 5;

#[derive(Clone)]
pub struct MessageCache {
    messages: Arc<Mutex<VecDeque<Message>>>,
}

impl MessageCache {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::with_capacity(CACHE_SIZE))),
        }
    }

    pub fn add(&mut self, msg: Message) {
        let mut messages = self.messages.lock().unwrap();
        if messages.len() == CACHE_SIZE {
            messages.pop_front();
        }
        messages.push_back(msg);
    }

    pub fn messages(&self) -> impl Iterator<Item = Message> {
        self.messages.lock().unwrap().clone().into_iter()
    }
}

impl Default for MessageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MessageCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MessageCache: [ ")?;
        self.messages
            .lock()
            .unwrap()
            .iter()
            .try_for_each(|msg| write!(f, "{}, ", msg.text().unwrap()))?;
        write!(f, "]")
    }
}
