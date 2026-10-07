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

    /// Count one execution that reserves no cycles.
    pub fn count(&mut self, now: u64, count_limit: u32) -> Result<()> {
        self.reserve(now, 0, count_limit, 0)
    }

    /// Replace a reservation made at `reserved_at` with what it actually
    /// charged, also returning its execution when nothing ran. A day that has
    /// since been reset no longer holds the reservation. Never traps, so a
    /// callback can settle after its call returns.
    pub fn settle(&mut self, reserved_at: u64, reserved: u128, charged: u128, executed: bool) {
        if self.day != reserved_at / DAY {
            return;
        }
        self.cycles = self.cycles.saturating_sub(reserved.saturating_sub(charged));
        if !executed {
            self.executions = self.executions.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_returns_the_unused_reservation_of_the_same_day() {
        let mut budget = Budget::default();
        budget.reserve(DAY + 1, 70, 3, 200).unwrap();
        budget.reserve(DAY + 2, 70, 3, 200).unwrap();
        // Two reservations fill the cycles; the first settles to its charge.
        assert_eq!(
            budget.reserve(DAY + 3, 70, 3, 200),
            Err(Error::QuotaExceeded)
        );
        budget.settle(DAY + 1, 70, 26, true);
        assert_eq!((budget.executions, budget.cycles), (2, 96));
        // A call that ran nothing returns its execution as well.
        budget.settle(DAY + 2, 70, 0, false);
        assert_eq!((budget.executions, budget.cycles), (1, 26));
        budget.reserve(DAY + 3, 70, 3, 200).unwrap();
        // The next day starts from zero; a late settlement leaves it alone.
        budget.reserve(2 * DAY, 70, 3, 200).unwrap();
        budget.settle(DAY + 3, 70, 0, false);
        assert_eq!((budget.day, budget.executions, budget.cycles), (2, 1, 70));
    }
}
