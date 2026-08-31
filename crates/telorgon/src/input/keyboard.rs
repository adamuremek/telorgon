//! Platform-neutral keyboard values.
//!
//! The physical and named-key vocabularies follow standardized keyboard meanings, but they are
//! owned by Telorgon. Platform adapters must translate native enums explicitly and map unknown
//! values to `Unidentified`; native numeric codes are diagnostic data, not portable identities.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::input::ButtonState;

/// Maximum UTF-8 byte length accepted for a logical character value or produced key text.
pub const MAX_KEY_TEXT_BYTES: usize = 4 * 1_024;

macro_rules! define_physical_key_codes {
    ($first:ident $(, $rest:ident)* $(,)?) => {
        /// Standardized identity for a known physical key position.
        ///
        /// These values describe the key's position rather than its layout-dependent meaning.
        /// New values are appended so the corresponding [`PhysicalKey`] identifier stays stable.
        #[repr(u16)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum PhysicalKeyCode {
            $first = 1,
            $($rest,)*
        }

        impl PhysicalKeyCode {
            /// Returns the stable nonzero Telorgon identifier for this standardized code.
            pub const fn portable_id(self) -> u32 {
                self as u32
            }

            /// Recovers a standardized code from a Telorgon physical-key identifier.
            pub const fn from_portable_id(value: u32) -> Option<Self> {
                match value {
                    value if value == Self::$first as u32 => Some(Self::$first),
                    $(value if value == Self::$rest as u32 => Some(Self::$rest),)*
                    _ => None,
                }
            }
        }
    };
}

define_physical_key_codes!(
    Backquote,
    Backslash,
    BracketLeft,
    BracketRight,
    Comma,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Equal,
    IntlBackslash,
    IntlRo,
    IntlYen,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Minus,
    Period,
    Quote,
    Semicolon,
    Slash,
    AltLeft,
    AltRight,
    Backspace,
    CapsLock,
    ContextMenu,
    ControlLeft,
    ControlRight,
    Enter,
    SuperLeft,
    SuperRight,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
    Convert,
    KanaMode,
    Lang1,
    Lang2,
    Lang3,
    Lang4,
    Lang5,
    NonConvert,
    Delete,
    End,
    Help,
    Home,
    Insert,
    PageDown,
    PageUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    NumLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadBackspace,
    NumpadClear,
    NumpadClearEntry,
    NumpadComma,
    NumpadDecimal,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    NumpadHash,
    NumpadMemoryAdd,
    NumpadMemoryClear,
    NumpadMemoryRecall,
    NumpadMemoryStore,
    NumpadMemorySubtract,
    NumpadMultiply,
    NumpadParenLeft,
    NumpadParenRight,
    NumpadStar,
    NumpadSubtract,
    Escape,
    Fn,
    FnLock,
    PrintScreen,
    ScrollLock,
    Pause,
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    Eject,
    LaunchApp1,
    LaunchApp2,
    LaunchMail,
    MediaPlayPause,
    MediaSelect,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrevious,
    Power,
    Sleep,
    AudioVolumeDown,
    AudioVolumeMute,
    AudioVolumeUp,
    WakeUp,
    Meta,
    Hyper,
    Turbo,
    Abort,
    Resume,
    Suspend,
    Again,
    Copy,
    Cut,
    Find,
    Open,
    Paste,
    Props,
    Select,
    Undo,
    Hiragana,
    Katakana,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    F26,
    F27,
    F28,
    F29,
    F30,
    F31,
    F32,
    F33,
    F34,
    F35,
);

/// Platform-neutral physical-key identity.
///
/// Zero is reserved for [`UNIDENTIFIED`](Self::UNIDENTIFIED). [`new`](Self::new) remains as the
/// compatibility seam for application-assigned physical identifiers; native adapters should use
/// [`from_code`](Self::from_code) for known keys and `UNIDENTIFIED` for everything else.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicalKey(u32);

impl PhysicalKey {
    /// A physical key for which no portable standardized position is known.
    pub const UNIDENTIFIED: Self = Self(0);

    /// Constructs an application-assigned neutral physical identity.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Constructs the portable identity for a standardized physical key code.
    pub const fn from_code(code: PhysicalKeyCode) -> Self {
        Self(code.portable_id())
    }

    /// Returns the standardized code when this value belongs to that vocabulary.
    pub const fn code(self) -> Option<PhysicalKeyCode> {
        PhysicalKeyCode::from_portable_id(self.0)
    }

    /// Returns the stable Telorgon identifier.
    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn is_unidentified(self) -> bool {
        self.0 == 0
    }
}

impl From<PhysicalKeyCode> for PhysicalKey {
    fn from(value: PhysicalKeyCode) -> Self {
        Self::from_code(value)
    }
}

/// Standardized layout-dependent meaning for a non-character key.
///
/// Character and dead-key meanings are represented separately by [`LogicalKey`]. This enum is
/// deliberately exhaustive for the current Telorgon vocabulary and contains no native key values.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NamedKey {
    Alt,
    AltGraph,
    CapsLock,
    Control,
    Fn,
    FnLock,
    NumLock,
    ScrollLock,
    Shift,
    Symbol,
    SymbolLock,
    Meta,
    Hyper,
    Super,
    Enter,
    Tab,
    Space,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    End,
    Home,
    PageDown,
    PageUp,
    Backspace,
    Clear,
    Copy,
    CrSel,
    Cut,
    Delete,
    EraseEof,
    ExSel,
    Insert,
    Paste,
    Redo,
    Undo,
    Accept,
    Again,
    Attn,
    Cancel,
    ContextMenu,
    Escape,
    Execute,
    Find,
    Help,
    Pause,
    Play,
    Props,
    Select,
    ZoomIn,
    ZoomOut,
    BrightnessDown,
    BrightnessUp,
    Eject,
    LogOff,
    Power,
    PowerOff,
    PrintScreen,
    Hibernate,
    Standby,
    WakeUp,
    AllCandidates,
    Alphanumeric,
    CodeInput,
    Compose,
    Convert,
    FinalMode,
    GroupFirst,
    GroupLast,
    GroupNext,
    GroupPrevious,
    ModeChange,
    NextCandidate,
    NonConvert,
    PreviousCandidate,
    Process,
    SingleCandidate,
    HangulMode,
    HanjaMode,
    JunjaMode,
    Eisu,
    Hankaku,
    Hiragana,
    HiraganaKatakana,
    KanaMode,
    KanjiMode,
    Katakana,
    Romaji,
    Zenkaku,
    ZenkakuHankaku,
    Soft1,
    Soft2,
    Soft3,
    Soft4,
    ChannelDown,
    ChannelUp,
    Close,
    MailForward,
    MailReply,
    MailSend,
    MediaClose,
    MediaFastForward,
    MediaPause,
    MediaPlay,
    MediaPlayPause,
    MediaRecord,
    MediaRewind,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrevious,
    New,
    Open,
    Print,
    Save,
    SpellCheck,
    Key11,
    Key12,
    AudioBalanceLeft,
    AudioBalanceRight,
    AudioBassBoostDown,
    AudioBassBoostToggle,
    AudioBassBoostUp,
    AudioFaderFront,
    AudioFaderRear,
    AudioSurroundModeNext,
    AudioTrebleDown,
    AudioTrebleUp,
    AudioVolumeDown,
    AudioVolumeUp,
    AudioVolumeMute,
    MicrophoneToggle,
    MicrophoneVolumeDown,
    MicrophoneVolumeUp,
    MicrophoneVolumeMute,
    SpeechCorrectionList,
    SpeechInputToggle,
    LaunchApplication1,
    LaunchApplication2,
    LaunchCalendar,
    LaunchContacts,
    LaunchMail,
    LaunchMediaPlayer,
    LaunchMusicPlayer,
    LaunchPhone,
    LaunchScreenSaver,
    LaunchSpreadsheet,
    LaunchWebBrowser,
    LaunchWebCam,
    LaunchWordProcessor,
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    AppSwitch,
    Call,
    Camera,
    CameraFocus,
    EndCall,
    GoBack,
    GoHome,
    HeadsetHook,
    LastNumberRedial,
    Notification,
    MannerMode,
    VoiceDial,
    TV,
    TV3DMode,
    TVAntennaCable,
    TVAudioDescription,
    TVAudioDescriptionMixDown,
    TVAudioDescriptionMixUp,
    TVContentsMenu,
    TVDataService,
    TVInput,
    TVInputComponent1,
    TVInputComponent2,
    TVInputComposite1,
    TVInputComposite2,
    TVInputHDMI1,
    TVInputHDMI2,
    TVInputHDMI3,
    TVInputHDMI4,
    TVInputVGA1,
    TVMediaContext,
    TVNetwork,
    TVNumberEntry,
    TVPower,
    TVRadioService,
    TVSatellite,
    TVSatelliteBS,
    TVSatelliteCS,
    TVSatelliteToggle,
    TVTerrestrialAnalog,
    TVTerrestrialDigital,
    TVTimer,
    AVRInput,
    AVRPower,
    ColorF0Red,
    ColorF1Green,
    ColorF2Yellow,
    ColorF3Blue,
    ColorF4Grey,
    ColorF5Brown,
    ClosedCaptionToggle,
    Dimmer,
    DisplaySwap,
    DVR,
    Exit,
    FavoriteClear0,
    FavoriteClear1,
    FavoriteClear2,
    FavoriteClear3,
    FavoriteRecall0,
    FavoriteRecall1,
    FavoriteRecall2,
    FavoriteRecall3,
    FavoriteStore0,
    FavoriteStore1,
    FavoriteStore2,
    FavoriteStore3,
    Guide,
    GuideNextDay,
    GuidePreviousDay,
    Info,
    InstantReplay,
    Link,
    ListProgram,
    LiveContent,
    Lock,
    MediaApps,
    MediaAudioTrack,
    MediaLast,
    MediaSkipBackward,
    MediaSkipForward,
    MediaStepBackward,
    MediaStepForward,
    MediaTopMenu,
    NavigateIn,
    NavigateNext,
    NavigateOut,
    NavigatePrevious,
    NextFavoriteChannel,
    NextUserProfile,
    OnDemand,
    Pairing,
    PinPDown,
    PinPMove,
    PinPToggle,
    PinPUp,
    PlaySpeedDown,
    PlaySpeedReset,
    PlaySpeedUp,
    RandomToggle,
    RcLowBattery,
    RecordSpeedNext,
    RfBypass,
    ScanChannelsToggle,
    ScreenModeNext,
    Settings,
    SplitScreenToggle,
    STBInput,
    STBPower,
    Subtitle,
    Teletext,
    VideoModeNext,
    Wink,
    ZoomToggle,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    F26,
    F27,
    F28,
    F29,
    F30,
    F31,
    F32,
    F33,
    F34,
    F35,
}

/// Owned, hard-bounded UTF-8 keyboard text.
///
/// Empty text is retained when a platform reports it. Debug output exposes only byte length so
/// typed characters cannot leak through diagnostics.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyText(Arc<str>);

impl KeyText {
    pub fn new(value: impl AsRef<str>) -> Result<Self, KeyTextError> {
        let value = value.as_ref();
        if value.len() > MAX_KEY_TEXT_BYTES {
            return Err(KeyTextError::TooLong {
                byte_len: value.len(),
                maximum_bytes: MAX_KEY_TEXT_BYTES,
            });
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn byte_len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for KeyText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyText")
            .field("byte_len", &self.byte_len())
            .field("redacted", &true)
            .finish()
    }
}

/// Failure to construct bounded [`KeyText`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum KeyTextError {
    TooLong {
        byte_len: usize,
        maximum_bytes: usize,
    },
}

impl fmt::Display for KeyTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong {
                byte_len,
                maximum_bytes,
            } => write!(
                formatter,
                "key text contains {byte_len} UTF-8 bytes; maximum is {maximum_bytes}"
            ),
        }
    }
}

impl Error for KeyTextError {}

/// Layout-dependent key meaning independent of physical position.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogicalKey {
    Character(KeyText),
    Named(NamedKey),
    Dead(Option<char>),
    #[default]
    Unidentified,
}

impl fmt::Debug for LogicalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(text) => formatter.debug_tuple("Character").field(text).finish(),
            Self::Named(key) => formatter.debug_tuple("Named").field(key).finish(),
            Self::Dead(character) => formatter
                .debug_struct("Dead")
                .field("has_character", &character.is_some())
                .field("character_redacted", &character.is_some())
                .finish(),
            Self::Unidentified => formatter.write_str("Unidentified"),
        }
    }
}

impl LogicalKey {
    pub fn character(value: impl AsRef<str>) -> Result<Self, KeyTextError> {
        KeyText::new(value).map(Self::Character)
    }
}

/// Physical location of a key when the platform can distinguish it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyLocation {
    #[default]
    Standard,
    Left,
    Right,
    Numpad,
}

/// Snapshot of portable keyboard modifiers and lock state.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Modifiers(u16);

impl Modifiers {
    pub const SHIFT: Self = Self(1 << 0);
    pub const CONTROL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);
    pub const ALT_GRAPH: Self = Self(1 << 4);
    pub const CAPS_LOCK: Self = Self(1 << 5);
    pub const NUM_LOCK: Self = Self(1 << 6);
    pub const SCROLL_LOCK: Self = Self(1 << 7);
    pub const FN: Self = Self(1 << 8);
    pub const FN_LOCK: Self = Self(1 << 9);
    pub const HYPER: Self = Self(1 << 10);
    pub const META: Self = Self(1 << 11);
    pub const SYMBOL: Self = Self(1 << 12);
    pub const SYMBOL_LOCK: Self = Self(1 << 13);
    pub const ALL: Self = Self((1 << 14) - 1);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn from_bits(bits: u16) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// Neutral keyboard value produced by a platform adapter.
///
/// `text` is informational input supplied outside an active IME composition. Editors must still
/// use IME commit as their only mutation path while composing, so this value cannot duplicate a
/// composition commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub physical_key: PhysicalKey,
    pub logical_key: LogicalKey,
    pub text: Option<KeyText>,
    pub location: KeyLocation,
    pub state: ButtonState,
    pub repeat: bool,
    pub synthetic: bool,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub const fn new(physical_key: PhysicalKey, state: ButtonState) -> Self {
        Self {
            physical_key,
            logical_key: LogicalKey::Unidentified,
            text: None,
            location: KeyLocation::Standard,
            state,
            repeat: false,
            synthetic: false,
            modifiers: Modifiers::empty(),
        }
    }

    pub fn with_logical_key(mut self, logical_key: LogicalKey) -> Self {
        self.logical_key = logical_key;
        self
    }

    pub fn with_text(mut self, text: Option<KeyText>) -> Self {
        self.text = text;
        self
    }

    pub fn with_location(mut self, location: KeyLocation) -> Self {
        self.location = location;
        self
    }

    pub fn with_repeat(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn with_synthetic(mut self, synthetic: bool) -> Self {
        self.synthetic = synthetic;
        self
    }

    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standardized_physical_codes_round_trip_without_native_values() {
        let key = PhysicalKey::from_code(PhysicalKeyCode::NumpadEnter);
        assert_eq!(key.code(), Some(PhysicalKeyCode::NumpadEnter));
        assert!(!key.is_unidentified());
        assert!(PhysicalKey::UNIDENTIFIED.is_unidentified());
        assert_eq!(PhysicalKey::UNIDENTIFIED.code(), None);
    }

    #[test]
    fn text_is_utf8_bounded_and_debug_redacted() {
        let text = KeyText::new("private-😀").unwrap();
        assert_eq!(text.as_str(), "private-😀");
        let debug = format!("{text:?}");
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("private"));

        let oversized = "x".repeat(MAX_KEY_TEXT_BYTES + 1);
        assert_eq!(
            KeyText::new(&oversized),
            Err(KeyTextError::TooLong {
                byte_len: MAX_KEY_TEXT_BYTES + 1,
                maximum_bytes: MAX_KEY_TEXT_BYTES,
            })
        );

        let dead = format!("{:?}", LogicalKey::Dead(Some('`')));
        assert!(dead.contains("character_redacted"));
        assert!(!dead.contains('`'));
    }

    #[test]
    fn event_preserves_complete_adapter_ready_keyboard_state() {
        let logical = LogicalKey::character("é").unwrap();
        let text = KeyText::new("é").unwrap();
        let modifiers = Modifiers::SHIFT
            .union(Modifiers::ALT_GRAPH)
            .union(Modifiers::CAPS_LOCK);
        let event = KeyEvent::new(
            PhysicalKey::from_code(PhysicalKeyCode::KeyE),
            ButtonState::Pressed,
        )
        .with_logical_key(logical.clone())
        .with_text(Some(text.clone()))
        .with_location(KeyLocation::Right)
        .with_repeat(true)
        .with_synthetic(true)
        .with_modifiers(modifiers);

        assert_eq!(event.logical_key, logical);
        assert_eq!(event.text.as_ref(), Some(&text));
        assert_eq!(event.location, KeyLocation::Right);
        assert!(event.repeat);
        assert!(event.synthetic);
        assert!(event.modifiers.contains(Modifiers::ALT_GRAPH));
        assert!(event.modifiers.contains(Modifiers::CAPS_LOCK));
    }

    #[test]
    fn modifiers_reject_unknown_bits() {
        assert_eq!(
            Modifiers::from_bits(Modifiers::SHIFT.union(Modifiers::NUM_LOCK).bits()),
            Some(Modifiers::SHIFT.union(Modifiers::NUM_LOCK))
        );
        assert_eq!(Modifiers::from_bits(1 << 15), None);
    }
}
