use std::hint::black_box;
use std::time::Instant;

use telorgon::{ChangeSource, NodeKind, Signal, ViewRuntime, app::*};

fn measure(name: &str, iterations: usize, mut scenario: impl FnMut() -> usize) {
    black_box(scenario());
    let started = Instant::now();
    let mut work = 0usize;
    for _ in 0..iterations {
        work ^= black_box(scenario());
    }
    let elapsed = started.elapsed();
    println!(
        "{name}: {:.2} us/iteration ({iterations} iterations, checksum {work})",
        elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64
    );
}

#[component]
struct InteractiveTree {
    #[input]
    buttons: usize,
    #[state]
    activations: u32,
}

impl Component for InteractiveTree {
    fn view(&self) -> impl View {
        let mut content = column();
        for index in 0..self.buttons {
            content = content.child(
                button(format!("Button {index}")).on_press(|this: &mut Self| this.activations += 1),
            );
        }
        content
    }
}

#[component(no_default)]
struct SignalTree {
    #[input]
    signals: Vec<Signal<u32>>,
}

impl Component for SignalTree {
    fn view(&self) -> impl View {
        let sum = self
            .signals
            .iter()
            .map(|signal| *self.watch(signal))
            .sum::<u32>();
        text(format!("Sum: {sum}"))
    }
}

fn main() {
    measure("composition_mount_buttons_1k", 25, || {
        let runtime = ViewRuntime::from_composed(InteractiveTree {
            buttons: 1_000,
            activations: 0,
        })
        .expect("interactive composition mounts");
        runtime.composition_diagnostics().elements_mounted as usize
    });

    let mut runtime = ViewRuntime::from_composed(InteractiveTree {
        buttons: 1_000,
        activations: 0,
    })
    .expect("interactive composition mounts");
    let target = runtime
        .ui()
        .nodes
        .alive()
        .iter()
        .copied()
        .find(|node| runtime.ui().kinds.get(*node) == Some(&NodeKind::Button))
        .expect("interactive tree contains a button");
    measure("composition_dispatch_reconcile_1k", 2_000, || {
        assert!(runtime.dispatch_activation(target, ChangeSource::Programmatic));
        runtime.composition_diagnostics().elements_reused as usize
    });

    measure("composition_watch_signals_16", 1_000, || {
        let signals = (0..16)
            .map(|value| Signal::new(value).0)
            .collect::<Vec<_>>();
        let runtime =
            ViewRuntime::from_composed(SignalTree { signals }).expect("signal composition mounts");
        runtime.composition_diagnostics().elements_mounted as usize
    });
}
