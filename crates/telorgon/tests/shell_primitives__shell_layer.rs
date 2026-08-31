use std::cell::RefCell;
use std::rc::Rc;

use telorgon::core::{EdgeInsets, RectF, SizeI};
use telorgon::runtime::{Component, CreateContext, Ui, UpdateContext, ViewRuntime};
use telorgon::shell::{
    OutputColorCapabilities, OutputGeometry, OutputId, OutputRevision, OutputSnapshot,
    OutputTransform, ShellCapabilities, ShellCapabilityGrant, ShellGrantToken, ShellLayerKind,
};
use telorgon::shell_primitives::prelude::{
    OutputView, ShellLayer, ShellLayerOrder, ShellLayerRef, ShellRoot,
};
use telorgon::ui::{BoxStyle, LayoutStyle, UiRoot};

struct Fixture(Rc<RefCell<Vec<ShellLayerRef>>>);

impl Component for Fixture {
    type State = ();
    type Action = ();

    fn create(&self, _: &mut CreateContext<'_>) -> Self::State {}

    fn mount(&self, _: &(), ui: &mut Ui<'_, '_, ()>) -> UiRoot {
        let host = ui
            .foundation()
            .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
        let output_id = OutputId::from_raw(1).unwrap();
        let grant = ShellCapabilityGrant::from_host(
            ShellGrantToken::from_raw(2).unwrap(),
            output_id,
            ShellCapabilities::BACKGROUND_LAYER | ShellCapabilities::PANEL_LAYER,
        );
        let root = ShellRoot::new("Mounted shell", grant)
            .unwrap()
            .mount(ui, host.0)
            .unwrap();
        let output = OutputView::new(OutputSnapshot::new(
            output_id,
            OutputRevision::INITIAL,
            OutputGeometry::new(
                RectF {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                RectF {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                SizeI {
                    width: 100,
                    height: 100,
                },
                1.0,
                OutputTransform::Normal,
                EdgeInsets::ZERO,
                OutputColorCapabilities::SRGB,
            )
            .unwrap(),
        ))
        .mount(ui, root)
        .unwrap();
        let mut order = ShellLayerOrder::new(output_id);
        for kind in [ShellLayerKind::Background, ShellLayerKind::Panel] {
            self.0.borrow_mut().push(
                ShellLayer::new(root.authorize_layer(kind).unwrap())
                    .mount(ui, output, &mut order)
                    .unwrap(),
            );
        }
        host
    }

    fn action(&self, _: &mut (), _: (), _: &mut UpdateContext<'_, Self>) {}
}

#[test]
fn public_layers_mount_in_authorized_canonical_order() {
    let layers = Rc::new(RefCell::new(Vec::new()));
    ViewRuntime::from_component(Fixture(layers.clone())).unwrap();
    let layers = layers.borrow();
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].kind(), ShellLayerKind::Background);
    assert_eq!(layers[1].kind(), ShellLayerKind::Panel);
    assert_eq!(layers[0].output(), layers[1].output());
}
