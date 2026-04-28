#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialDecl {
    pub name: String,
    pub kind: MaterialKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialKind {
    SolidColor,
    BackdropBlur,
    GlassTint,
}
