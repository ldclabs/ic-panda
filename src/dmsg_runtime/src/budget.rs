use dmsg_types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Budget {
    pub day: u64,
    pub executions: u32,
    pub cycles: u128,
}

impl Budget {
    pub fn reserve(
        &mut self,
        now: u64,
        cycles: u128,
        count_limit: u32,
        cycle_limit: u128,
    ) -> Result<()> {
        let mut next = self.clone();
        if now / DAY > next.day {
            next = Self {
                day: now / DAY,
                ..Self::default()
            };
        }
        next.executions = next.executions.checked_add(1).ok_or(Error::QuotaExceeded)?;
        next.cycles = next
            .cycles
            .checked_add(cycles)
            .ok_or(Error::QuotaExceeded)?;
        ensure(
            next.executions <= count_limit && next.cycles <= cycle_limit,
            Error::QuotaExceeded,
        )?;
        *self = next;
        Ok(())
    }
}
