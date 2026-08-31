use telorgon::platform::MonotonicInstant;
use telorgon::platform::clock::{MonotonicClock, MonotonicClockState};

#[derive(Debug)]
struct ScriptedClock {
    samples: [MonotonicInstant; 3],
    next: usize,
}

impl MonotonicClock for ScriptedClock {
    fn now(&mut self) -> MonotonicInstant {
        let sample = self.samples[self.next];
        self.next += 1;
        sample
    }
}

#[test]
fn public_clock_path_accepts_ties_and_rejects_regression_atomically() {
    let mut state = MonotonicClockState::new(ScriptedClock {
        samples: [
            MonotonicInstant::from_nanos(30),
            MonotonicInstant::from_nanos(29),
            MonotonicInstant::from_nanos(30),
        ],
        next: 0,
    });

    assert_eq!(state.observe_now().unwrap().as_nanos(), 30);
    let error = state.observe_now().unwrap_err();
    assert_eq!(error.previous().as_nanos(), 30);
    assert_eq!(error.observed().as_nanos(), 29);
    assert_eq!(state.last_observed().unwrap().as_nanos(), 30);
    assert_eq!(state.observe_now().unwrap().as_nanos(), 30);
}
