use std::collections::{HashMap, VecDeque};
use teloxide::prelude::{ChatId, Message};

pub struct MessageCache {
    messages: HashMap<ChatId, VecDeque<Message>>,
    size: usize,
}

impl MessageCache {
    pub fn new(size: usize) -> Self {
        Self {
            messages: HashMap::new(),
            size,
        }
    }

    pub fn add(&mut self, msg: Message) {
        if self.size > 0 {
            let chat_id = msg.chat.id;
            let messages = self
                .messages
                .entry(chat_id)
                .or_insert(VecDeque::with_capacity(self.size));
            if messages.len() == self.size {
                messages.pop_front();
            }
            messages.push_back(msg);
        }
    }

    pub fn messages(&mut self, chat_id: ChatId) -> impl Iterator<Item = &Message> {
        self.messages
            .entry(chat_id)
            .or_insert(VecDeque::with_capacity(self.size))
            .iter()
    }
}
