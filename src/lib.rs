pub mod device;
pub mod errors;
pub mod headers;
pub mod interface;
pub mod message;

pub const MASTER_ADDR: u8 = 0x01;
pub const DEFAULT_TIMEOUT_MS: u32 = 40;
