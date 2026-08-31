use telorgon::{
    ComponentStyleContract, StylePropertyMask, StylePropertyPatch, ThemeCatalog, ThemeDomain,
    ThemeRuntime, ThemeScopeKind, ThemeSource, theme,
};

fn catalog(domain: ThemeDomain, component: &str) -> ThemeCatalog {
    let mut catalog = ThemeCatalog::new(domain);
    catalog
        .register(
            ComponentStyleContract::new(component)
                .slot("root", StylePropertyMask::ALL)
                .style(
                    "default",
                    [("root".to_owned(), StylePropertyPatch::default())],
                ),
        )
        .unwrap();
    catalog
}

fn compile(domain: ThemeDomain, component: &str) -> theme::CompiledTheme {
    let source = ThemeSource::parse(&format!(
        "format='v4'\ndomain='{}'\n[components.{component}.default.slots.root]\nradius=8",
        domain.name()
    ))
    .unwrap();
    theme::CompiledTheme::compile(&source, &catalog(domain, component)).unwrap()
}

#[test]
fn umbrella_exposes_isolated_application_shell_and_preview_theme_scopes() {
    let application_theme = compile(ThemeDomain::Application, "button");
    let shell_theme = compile(ThemeDomain::Shell, "window");
    let mut runtime = ThemeRuntime::new(application_theme, shell_theme).unwrap();
    let application = ThemeRuntime::root_scope(ThemeDomain::Application);
    let shell = ThemeRuntime::root_scope(ThemeDomain::Shell);
    assert_ne!(application.id(), shell.id());
    assert!(
        runtime
            .theme(application)
            .unwrap()
            .style_id("button", "default")
            .is_some()
    );
    assert!(
        runtime
            .theme(application)
            .unwrap()
            .style_id("window", "default")
            .is_none()
    );
    assert!(
        runtime
            .theme(shell)
            .unwrap()
            .style_id("window", "default")
            .is_some()
    );

    let preview = runtime
        .create_preview(compile(ThemeDomain::Application, "preview"))
        .unwrap();
    assert_eq!(preview.kind(), ThemeScopeKind::Preview);
    assert!(
        runtime
            .theme(preview)
            .unwrap()
            .style_id("preview", "default")
            .is_some()
    );
    assert!(
        runtime
            .theme(application)
            .unwrap()
            .style_id("preview", "default")
            .is_none()
    );
    assert!(runtime.discard_preview(preview));
    assert!(runtime.theme(preview).is_none());

    let _: ThemeRuntime = ThemeRuntime::default();
    let _: Option<theme::CompiledTheme> = None;
}
