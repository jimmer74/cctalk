use core::ops::Deref;

#[derive(Default, Debug, Clone)]
pub struct EventCounter(u8);

#[allow(unused)]
impl EventCounter {
    fn increase(&mut self) {
        if self.0 == 255 {
            self.0 = 1;
        } else {
            self.0 += 1;
        }
    }
    fn set(&mut self, value: u8) {
        self.0 = value;
    }
    fn get(&self) -> u8 {
        self.0
    }
}

impl Deref for EventCounter {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
