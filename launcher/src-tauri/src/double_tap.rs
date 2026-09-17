//! Deterministic gesture recognition, independent of hooks and wall-clock time.
#[derive(Default)]
pub struct DoubleTap {
    first: Option<(u16, u32, usize)>,
    down: Option<(u16, u32, usize, bool)>,
}

impl DoubleTap {
    pub fn reset(&mut self) {
        self.first = None;
        self.down = None;
    }

    pub fn press(&mut self, key: u16, time: u32, window: usize, threshold: u32) {
        if self.down.is_some() {
            return; // OS auto-repeat is not a second press.
        }
        let second = self.first.is_some_and(|(previous, start, target)| {
            previous == key && target == window && time.wrapping_sub(start) <= threshold
        });
        self.down = Some((key, time, window, second));
    }

    pub fn release(&mut self, key: u16, time: u32, window: usize, threshold: u32) -> bool {
        let Some((pressed, start, target, second)) = self.down.take() else {
            return false;
        };
        if key != pressed || window != target || time.wrapping_sub(start) > threshold {
            self.reset();
            return false;
        }
        if second {
            self.reset();
            true
        } else {
            self.first = Some((key, start, window));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(detector: &mut DoubleTap, key: u16, time: u32, window: usize) -> bool {
        detector.press(key, time, window, 200);
        detector.release(key, time.wrapping_add(20), window, 200)
    }

    #[test]
    fn threshold_is_inclusive_and_requires_release() {
        for (delta, expected) in [(199, true), (200, true), (201, false)] {
            let mut d = DoubleTap::default();
            assert!(!tap(&mut d, 91, 1000, 1));
            assert_eq!(tap(&mut d, 91, 1000 + delta, 1), expected);
        }
        let mut d = DoubleTap::default();
        d.press(91, 0, 1, 200);
        d.press(91, 100, 1, 200);
        assert!(!d.release(91, 120, 1, 200));
    }

    #[test]
    fn different_keys_windows_and_interrupted_gestures_do_not_match() {
        for (key, window) in [(92, 1), (91, 2)] {
            let mut d = DoubleTap::default();
            tap(&mut d, 91, 0, 1);
            assert!(!tap(&mut d, key, 100, window));
        }
        let mut d = DoubleTap::default();
        tap(&mut d, 91, 0, 1);
        d.reset();
        assert!(!tap(&mut d, 91, 100, 1));
    }

    #[test]
    fn triple_tap_fires_once_and_holding_does_not_fire() {
        let mut d = DoubleTap::default();
        assert!(!tap(&mut d, 91, 0, 1));
        assert!(tap(&mut d, 91, 100, 1));
        assert!(!tap(&mut d, 91, 150, 1));
        d.press(91, 220, 1, 200);
        assert!(!d.release(91, 500, 1, 200));
    }

    #[test]
    fn supports_wrapping_windows_event_timestamps() {
        let mut d = DoubleTap::default();
        assert!(!tap(&mut d, 91, u32::MAX - 50, 1));
        assert!(tap(&mut d, 91, 30, 1));
    }
}
