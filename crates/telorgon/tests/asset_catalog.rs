use telorgon::{AssetKind, asset_catalog};

asset_catalog! { mod fixture_assets = "tests/fixtures/assets"; }

#[test]
fn generated_catalog_exposes_nested_typed_assets_and_bundle() {
    let bundle = fixture_assets::bundle().validate().unwrap();
    assert_eq!(bundle.len(), 4);
    assert_eq!(
        fixture_assets::icons::APP.resolve(bundle).unwrap().kind,
        AssetKind::Icon
    );
    assert_eq!(
        fixture_assets::images::HERO.resolve(bundle).unwrap().kind,
        AssetKind::Image
    );
    assert_eq!(
        fixture_assets::cursors::POINTER
            .resolve(bundle)
            .unwrap()
            .kind,
        AssetKind::Cursor
    );
    assert_eq!(
        fixture_assets::cursors::DEFAULT
            .resolve(bundle)
            .unwrap()
            .kind,
        AssetKind::CursorTheme
    );
}
