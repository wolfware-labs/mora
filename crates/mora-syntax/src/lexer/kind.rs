//! Token kinds (§3.4, §3.6).

/// Declares [`TokenKind`]: the kinds without fixed text, then the
/// punctuation and the reserved keywords, each with its text.
macro_rules! kinds {
    (
        other: { $($(#[doc = $doc:literal])* $other:ident,)* }
        punct: { $($punct:ident = $ptext:literal,)* }
        keywords: { $($kw:ident = $ktext:literal,)* }
    ) => {
        /// The kind of a token. Trivia (whitespace, newlines, comments and
        /// directives) are tokens too, so the tokens of a file cover all of
        /// its text.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum TokenKind {
            $($(#[doc = $doc])* $other,)*
            $(#[doc = concat!("`", $ptext, "`")] $punct,)*
            $(#[doc = concat!("The keyword `", $ktext, "`.")] $kw,)*
        }

        impl TokenKind {
            /// The fixed text of a punctuation or keyword token.
            pub fn text(self) -> Option<&'static str> {
                match self {
                    $(TokenKind::$punct => Some($ptext),)*
                    $(TokenKind::$kw => Some($ktext),)*
                    _ => None,
                }
            }

            /// The reserved keyword spelled `text`, if any (§3.4). Contextual
            /// keywords are identifiers.
            pub fn keyword(text: &str) -> Option<TokenKind> {
                match text {
                    $($ktext => Some(TokenKind::$kw),)*
                    _ => None,
                }
            }

            /// Whether this is a reserved keyword.
            pub fn is_keyword(self) -> bool {
                matches!(self, $(TokenKind::$kw)|*)
            }

            /// Every punctuation kind, for tests.
            pub const PUNCTUATION: &[TokenKind] = &[$(TokenKind::$punct),*];

            /// Every reserved keyword kind, for tests.
            pub const KEYWORDS: &[TokenKind] = &[$(TokenKind::$kw),*];
        }
    };
}

kinds! {
    other: {
        /// Spaces, tabs, a lone carriage return, a form feed or a byte order mark.
        Whitespace,
        /// `\n` or `\r\n`.
        Newline,
        /// `//` to the end of the line.
        LineComment,
        /// `///` to the end of the line.
        DocComment,
        /// `/* ... */`.
        BlockComment,
        /// A line starting with `#` and a directive name, such as `#region` (§3.2).
        Directive,
        /// A name, possibly with an `@` prefix, including contextual keywords.
        Ident,
        /// An integer literal, such as `0x4D2` or `10ul`.
        IntNumber,
        /// A real literal, such as `1.5e-3f`.
        RealNumber,
        /// A character literal, such as `'a'`.
        Char,
        /// A regular string literal, such as `"a\tb"` or `"abc"u8`.
        String,
        /// A verbatim string literal, such as `@"C:\temp"`.
        VerbatimString,
        /// A raw string literal, delimited by three or more quotes.
        RawString,
        /// The start of an interpolated string, such as `$"`, `$@"` or `$$"""`.
        InterpStart,
        /// Literal text inside an interpolated string.
        InterpText,
        /// The `{` (or `{{` after `$$`) that opens a hole.
        HoleOpen,
        /// The `:format` part of a hole, from the colon to the closing brace.
        HoleFormat,
        /// The `}` (or `}}` after `$$`) that closes a hole.
        HoleClose,
        /// The quotes that end an interpolated string.
        InterpEnd,
        /// A character that cannot start a token.
        Unknown,
    }
    punct: {
    LBrace = "{",
    RBrace = "}",
    LBracket = "[",
    RBracket = "]",
    LParen = "(",
    RParen = ")",
    Dot = ".",
    DotDot = "..",
    Comma = ",",
    Colon = ":",
    ColonColon = "::",
    Semicolon = ";",
    Question = "?",
    QuestionQuestion = "??",
    QuestionQuestionEq = "??=",
    QuestionDot = "?.",
    FatArrow = "=>",
    ThinArrow = "->",
    At = "@",
    Dollar = "$",
    Hash = "#",
    Plus = "+",
    Minus = "-",
    Star = "*",
    Slash = "/",
    Percent = "%",
    Amp = "&",
    Pipe = "|",
    Caret = "^",
    Bang = "!",
    Tilde = "~",
    Shl = "<<",
    EqEq = "==",
    BangEq = "!=",
    Lt = "<",
    Gt = ">",
    LtEq = "<=",
    AmpAmp = "&&",
    PipePipe = "||",
    Eq = "=",
    PlusEq = "+=",
    MinusEq = "-=",
    StarEq = "*=",
    SlashEq = "/=",
    PercentEq = "%=",
    AmpEq = "&=",
    PipeEq = "|=",
    CaretEq = "^=",
    ShlEq = "<<=",
    PlusPlus = "++",
    MinusMinus = "--",
    PlusPercent = "+%",
    MinusPercent = "-%",
    StarPercent = "*%",
    PlusPercentEq = "+%=",
    MinusPercentEq = "-%=",
    StarPercentEq = "*%=",
    }
    keywords: {
    AbstractKw = "abstract",
    AsKw = "as",
    AsyncKw = "async",
    AwaitKw = "await",
    BaseKw = "base",
    BoolKw = "bool",
    BreakKw = "break",
    ByteKw = "byte",
    CaseKw = "case",
    CatchKw = "catch",
    CharKw = "char",
    ClassKw = "class",
    ConstKw = "const",
    ContinueKw = "continue",
    DefaultKw = "default",
    DelegateKw = "delegate",
    DoKw = "do",
    DoubleKw = "double",
    DynamicKw = "dynamic",
    ElseKw = "else",
    EnumKw = "enum",
    EventKw = "event",
    ExplicitKw = "explicit",
    ExternKw = "extern",
    FalseKw = "false",
    FinallyKw = "finally",
    FixedKw = "fixed",
    FloatKw = "float",
    ForKw = "for",
    ForeachKw = "foreach",
    GotoKw = "goto",
    IfKw = "if",
    ImplicitKw = "implicit",
    ImportKw = "import",
    InKw = "in",
    IndirectKw = "indirect",
    IntKw = "int",
    InterfaceKw = "interface",
    InternalKw = "internal",
    IsKw = "is",
    LazyKw = "lazy",
    LetKw = "let",
    LockKw = "lock",
    LongKw = "long",
    MacroKw = "macro",
    MoveKw = "move",
    MutKw = "mut",
    NamespaceKw = "namespace",
    NewKw = "new",
    NintKw = "nint",
    NuintKw = "nuint",
    NullKw = "null",
    ObjectKw = "object",
    OperatorKw = "operator",
    OutKw = "out",
    OverrideKw = "override",
    ParamsKw = "params",
    PrivateKw = "private",
    ProtectedKw = "protected",
    PublicKw = "public",
    ReadonlyKw = "readonly",
    RefKw = "ref",
    ReturnKw = "return",
    SbyteKw = "sbyte",
    SealedKw = "sealed",
    SharedKw = "shared",
    ShortKw = "short",
    SizeofKw = "sizeof",
    StackallocKw = "stackalloc",
    StaticKw = "static",
    StringKw = "string",
    StructKw = "struct",
    SwitchKw = "switch",
    ThisKw = "this",
    ThrowKw = "throw",
    TrueKw = "true",
    TryKw = "try",
    TypeofKw = "typeof",
    UintKw = "uint",
    UlongKw = "ulong",
    UnionKw = "union",
    UniqueKw = "unique",
    UnsafeKw = "unsafe",
    UshortKw = "ushort",
    UsingKw = "using",
    VarKw = "var",
    VirtualKw = "virtual",
    VoidKw = "void",
    VolatileKw = "volatile",
    WeakKw = "weak",
    WhereKw = "where",
    WhileKw = "while",
    YieldKw = "yield",
    }
}

impl TokenKind {
    /// Whether the parser skips this token: whitespace, newlines, comments
    /// and directives.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::Newline
                | TokenKind::LineComment
                | TokenKind::DocComment
                | TokenKind::BlockComment
                | TokenKind::Directive
        )
    }
}
