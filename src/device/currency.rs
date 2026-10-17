pub type CurrancySlots = [u8; 16];
pub type ScalingFactor = [u8; 3];

#[derive(Debug, Clone, Copy)]
pub struct Currancy {
    pub slots: CurrancySlots,
    pub sf: ScalingFactor,
    pub cc: [u8; 2],
    pub rev: [u8; 256],
}

impl Default for Currancy {
    fn default() -> Self {
        Self {
            slots: CurrancySlots::default(),
            sf: ScalingFactor::default(),
            cc: [0u8; 2],
            rev: [0u8; 256],
        }
    }
}

#[allow(unused)]
impl Currancy {
    pub fn slot_to_currancy(&self, slot: u8) -> f32 {
        let note_val = self.slots[slot as usize] as f32;
        let multiplier = self.sf[0] as f32; //to get value of currency in minor units eg pence
        let sf = (10_u32.pow(self.sf[1] as u32)) as f32; //value of currancy relative to minor unit
        let dp = (10_u32.pow(self.sf[2] as u32)) as f32; //divisor to get final currency in terms of major
        //units (eg pounds)

        // println!(
        //     "note_val: {}, multiplier: {}, sf: {}, dp: {} ",
        //     note_val, multiplier, sf, dp
        // );
        (note_val * multiplier * sf) / dp
    }

    pub fn is_occupied(&self, index: usize) -> bool {
        self.slots[index] != 0
    }

    pub fn num_slots(&self) -> usize {
        let num_slots = self.slots.iter().filter(|slot| *slot != &0u8).count();
        num_slots
    }
}
