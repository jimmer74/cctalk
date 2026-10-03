use core::ops::Deref;

#[derive(Default, Debug, Clone)]
pub struct EventCounter(u8);

#[allow(unused)]
impl EventCounter {
    pub fn new(value: u8) -> Self {
        EventCounter(value)
    }

    pub fn increase(&mut self) {
        if self.0 == 255 {
            self.0 = 1;
        } else {
            self.0 += 1;
        }
    }
    pub fn set(&mut self, value: u8) {
        self.0 = value;
    }
    pub fn get(&self) -> u8 {
        self.0
    }

    pub fn diff(&self, other: &Self) -> u8 {
        self.0 - other.0
    }
}

impl Deref for EventCounter {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
