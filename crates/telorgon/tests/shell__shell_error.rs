use telorgon::shell::{ShellError, ShellErrorKind, ShellRequestResult};

#[test]
fn public_shell_errors_are_typed_and_redaction_safe() {
    let error = ShellError::from_rejection(ShellRequestResult::Denied, "public fixture").unwrap();
    assert_eq!(error.kind(), ShellErrorKind::RequestDenied);
    assert_eq!(error.context(), "public fixture");
}
