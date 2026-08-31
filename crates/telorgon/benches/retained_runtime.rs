use std::hint::black_box;
use std::time::Instant;

use telorgon::{
    Background, BoxStyle, ColorRgba8, LayoutEngine, LayoutStyle, MountWriter, MountedUi,
    RenderScene, RetainedTextRequest, RetainedTextSystem, SceneCompiler, SizeF, SizeRule,
    TextRunKey, VirtualCollection,
};

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

fn main() {
    measure("static_mount_10k", 5, || {
        let mut ui = MountedUi::default();
        let mut builder = MountWriter::<()>::new(&mut ui);
        builder.root(BoxStyle::default(), LayoutStyle::default(), |builder| {
            for _ in 0..10_000 {
                builder.container(BoxStyle::default(), LayoutStyle::default(), |_| {});
            }
        });
        ui.memory_report().total_bytes()
    });

    let mut ui = MountedUi::default();
    let mut controls = Vec::with_capacity(10_000);
    {
        let mut builder = MountWriter::new(&mut ui);
        builder.root(BoxStyle::default(), LayoutStyle::default(), |builder| {
            for _ in 0..10_000 {
                controls.push(builder.button(
                    (),
                    BoxStyle {
                        width: SizeRule::Px(10.0),
                        height: SizeRule::Px(1.0),
                        background: Background::Color(ColorRgba8::rgba(24, 48, 96, 255)),
                        ..BoxStyle::default()
                    },
                    |_| {},
                ));
            }
        });
    }
    let extent = SizeF {
        width: 100.0,
        height: 10_000.0,
    };
    let mut layout = LayoutEngine::default();
    let mut text = RetainedTextSystem::new(4096).expect("text system");
    let mut scene = RenderScene::default();
    let mut compiler = SceneCompiler::default();
    layout.update(&mut ui, &mut text, extent, 1.0);
    compiler.compile(
        &mut ui,
        &layout,
        &mut text,
        &mut scene,
        extent,
        ColorRgba8::default(),
    );
    scene.take_delta();
    let mut tick = 0usize;
    measure("interactive_patch_10k", 2_000, || {
        tick += 1;
        let index = tick % controls.len();
        ui.transaction(|transaction| {
            transaction.set(
                controls[index].opacity,
                if tick & 1 == 0 { 0.5 } else { 0.75 },
            );
        });
        layout.update(&mut ui, &mut text, extent, 1.0);
        let stats = compiler.compile(
            &mut ui,
            &layout,
            &mut text,
            &mut scene,
            extent,
            ColorRgba8::default(),
        );
        scene.take_delta();
        stats.visited as usize
    });

    let animated = &controls[..1_000];
    measure("animated_progress_1k", 200, || {
        tick += 1;
        ui.transaction(|transaction| {
            for (index, control) in animated.iter().enumerate() {
                transaction.set(control.value, ((tick + index) % 100) as f32 / 100.0);
            }
        });
        layout.update(&mut ui, &mut text, extent, 1.0);
        let stats = compiler.compile(
            &mut ui,
            &layout,
            &mut text,
            &mut scene,
            extent,
            ColorRgba8::default(),
        );
        scene.take_delta();
        stats.visited as usize
    });

    let mut collection = VirtualCollection::new(100_000, 20.0, 200.0);
    for index in (0..100_000).step_by(97) {
        collection.set_extent(index, 12.0 + (index % 71) as f32);
    }
    measure("virtual_list_100k", 10_000, || {
        tick += 1;
        let offset = (tick as f32 * 37.0) % collection.total_extent();
        let range = collection.visible_range(offset, 800.0);
        range.end - range.start
    });

    let mut editor_text = RetainedTextSystem::new(100_000).expect("text system");
    measure("editor_retained_text", 2_000, || {
        tick += 1;
        let revision = (tick % 32) as u64 + 1;
        let key = TextRunKey::new(
            revision,
            1,
            "sans-serif",
            14.0,
            400,
            18.0,
            Some(800.0),
            Some(24.0),
            1.0,
        );
        editor_text
            .prepare(RetainedTextRequest {
                key,
                text: "fn retained_editor_line() { /* shaped once per revision */ }",
                family: "sans-serif",
                font_size_px: 14,
                line_height_px: 18,
                max_width_px: Some(800.0),
                max_height_px: Some(24.0),
            })
            .expect("shape retained text")
            .0 as usize
    });
}
