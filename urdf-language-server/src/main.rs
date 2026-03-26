use regex::Regex;
use std::collections::{BTreeSet, HashMap};
use tokio::sync::RwLock;
use tower_lsp::async_trait;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Known URDF element names used for tag completions.
///
/// This list is intentionally small and focused on common URDF tags.
/// You can extend it later without changing the completion engine itself.
const TAGS: &[&str] = &[
    "robot",
    "link",
    "joint",
    "visual",
    "collision",
    "inertial",
    "origin",
    "geometry",
    "mesh",
    "box",
    "cylinder",
    "sphere",
    "material",
    "color",
    "texture",
    "parent",
    "child",
    "axis",
    "limit",
    "dynamics",
    "mass",
    "inertia",
];

/// Known URDF joint types used for value completions.
const JOINT_TYPES: &[&str] = &[
    "revolute",
    "continuous",
    "prismatic",
    "fixed",
    "floating",
    "planar",
];

/// A byte-based replacement range in the document.
///
/// We use byte offsets internally because the completion engine first
/// analyzes the raw source text, then converts offsets back into LSP ranges.
#[derive(Clone, Copy, Debug)]
struct ByteRange {
    start: usize,
    end: usize,
}

/// The kind of tag-name context currently being completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TagNameContextKind {
    /// Completing an opening tag, e.g. `<ro|`
    Opening,

    /// Completing a closing tag, e.g. `</ro|`
    Closing,
}

/// Context in which a completion request was triggered.
enum CompletionContext {
    /// The cursor is currently inside a tag name.
    ///
    /// `replace_range` is the exact document range to replace. This is what
    /// fixes the extra `>` problem caused by editor auto-pairing.
    TagName {
        fragment: String,
        kind: TagNameContextKind,
        replace_range: ByteRange,
    },

    /// The cursor is currently typing an attribute name inside a tag.
    AttributeName {
        tag: String,
        fragment: String,
        replace_range: ByteRange,
    },

    /// The cursor is currently typing an attribute value inside a tag.
    AttributeValue {
        tag: String,
        attribute: String,
        fragment: String,
        replace_range: ByteRange,
    },
}

/// Lightweight in-memory backend.
///
/// This server intentionally focuses on completion rather than full validation.
/// It stores the latest text for each open document and derives completions
/// from the current tag / attribute / attribute-value context.
struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "urdf-language-server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "<".to_string(),
                        "/".to_string(),
                        " ".to_string(),
                        "=".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "URDF language server initialized successfully.",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.write().await;

        if let Some(change) = params.content_changes.into_iter().next() {
            docs.insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(Some(CompletionResponse::Array(vec![])));
        };

        let offset = offset_of_position(text, position);
        let items = build_completions(text, offset);

        Ok(Some(CompletionResponse::Array(items)))
    }
}

/// Convert an LSP position into a byte offset.
///
/// LSP positions are UTF-16 based, so this helper carefully walks the line
/// and counts UTF-16 code units.
fn offset_of_position(text: &str, position: Position) -> usize {
    let mut byte_offset = 0usize;
    let mut current_line = 0u32;

    for line in text.split_inclusive('\n') {
        if current_line == position.line {
            let mut utf16_units = 0u32;
            let mut bytes_in_line = 0usize;

            for ch in line.chars() {
                if utf16_units >= position.character {
                    break;
                }

                utf16_units += ch.len_utf16() as u32;
                bytes_in_line += ch.len_utf8();
            }

            return byte_offset + bytes_in_line.min(line.len());
        }

        byte_offset += line.len();
        current_line += 1;
    }

    text.len()
}

/// Convert a byte offset back into an LSP position.
///
/// This is required because completion items that use `text_edit` must provide
/// an LSP `Range` instead of raw byte offsets.
fn position_of_offset(text: &str, target: usize) -> Position {
    let target = target.min(text.len());

    let mut line = 0u32;
    let mut character = 0u32;
    let mut offset = 0usize;

    for ch in text.chars() {
        let len = ch.len_utf8();

        if offset + len > target {
            break;
        }

        offset += len;

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position { line, character }
}

/// Convert a byte range into an LSP range.
fn lsp_range_from_byte_range(text: &str, byte_range: ByteRange) -> Range {
    Range {
        start: position_of_offset(text, byte_range.start),
        end: position_of_offset(text, byte_range.end),
    }
}

/// Build completion items from the current syntactic context.
fn build_completions(text: &str, offset: usize) -> Vec<CompletionItem> {
    match detect_context(text, offset) {
        Some(CompletionContext::TagName {
            fragment,
            kind,
            replace_range,
        }) => match kind {
            TagNameContextKind::Opening => {
                complete_opening_tag_names(text, &fragment, replace_range)
            }
            TagNameContextKind::Closing => {
                complete_closing_tag_names(text, &fragment, replace_range)
            }
        },

        Some(CompletionContext::AttributeName {
            tag,
            fragment,
            replace_range,
        }) => complete_attribute_names(text, &tag, &fragment, replace_range),

        Some(CompletionContext::AttributeValue {
            tag,
            attribute,
            fragment,
            replace_range,
        }) => complete_attribute_values(text, &tag, &attribute, &fragment, replace_range),

        None => vec![],
    }
}

/// Detect whether the cursor is currently typing a tag name,
/// an attribute name, or an attribute value.
fn detect_context(text: &str, offset: usize) -> Option<CompletionContext> {
    let prefix = &text[..offset.min(text.len())];

    let open_angle = prefix.rfind('<')?;
    let close_angle = prefix.rfind('>');

    // If the latest `>` is after the latest `<`, then the cursor is not inside
    // a tag anymore, so no tag/attribute completion should be offered.
    if let Some(close_angle) = close_angle {
        if close_angle > open_angle {
            return None;
        }
    }

    let tag_region = &text[open_angle..offset.min(text.len())];

    // Ignore declarations, comments, and processing instructions.
    if tag_region.starts_with("<!--")
        || tag_region.starts_with("<?")
        || tag_region.starts_with("<!")
    {
        return None;
    }

    if let Some((kind, fragment, fragment_start_rel)) = detect_tag_name_fragment(tag_region) {
        let replace_start = open_angle + fragment_start_rel;
        let replace_end = tag_name_replace_end(text, offset);

        return Some(CompletionContext::TagName {
            fragment,
            kind,
            replace_range: ByteRange {
                start: replace_start,
                end: replace_end,
            },
        });
    }

    if let Some((tag, attribute, fragment, fragment_start_rel)) =
        detect_attribute_value_context(tag_region)
    {
        return Some(CompletionContext::AttributeValue {
            tag,
            attribute,
            fragment,
            replace_range: ByteRange {
                start: open_angle + fragment_start_rel,
                end: offset,
            },
        });
    }

    let tag = current_tag_name(tag_region)?;
    let (fragment, fragment_start_rel) = current_attribute_name_fragment(tag_region);

    Some(CompletionContext::AttributeName {
        tag,
        fragment,
        replace_range: ByteRange {
            start: open_angle + fragment_start_rel,
            end: offset,
        },
    })
}

/// Detect whether the user is typing directly after `<` or `</`.
///
/// Examples:
/// - `<ro|`   -> opening tag name, fragment = "ro"
/// - `</jo|`  -> closing tag name, fragment = "jo"
fn detect_tag_name_fragment(tag_region: &str) -> Option<(TagNameContextKind, String, usize)> {
    let (kind, after_prefix, fragment_start_rel) =
        if let Some(stripped) = tag_region.strip_prefix("</") {
            (TagNameContextKind::Closing, stripped, 2usize)
        } else if let Some(stripped) = tag_region.strip_prefix('<') {
            (TagNameContextKind::Opening, stripped, 1usize)
        } else {
            return None;
        };

    if after_prefix.is_empty() {
        return Some((kind, String::new(), fragment_start_rel));
    }

    let fragment: String = after_prefix
        .chars()
        .take_while(|c| is_name_char(*c))
        .collect();

    // We only treat the cursor as still being inside the tag name if
    // everything after `<` or `</` is part of the current partial name.
    if fragment == after_prefix {
        Some((kind, fragment, fragment_start_rel))
    } else {
        None
    }
}

/// Compute the replacement end for tag-name completions.
///
/// This is the key fix for the extra `>` issue:
/// when the editor has already auto-inserted `>` or `/>` to the right of the
/// cursor, the completion should replace that delimiter instead of leaving it
/// behind and producing duplicated closing punctuation.
fn tag_name_replace_end(text: &str, offset: usize) -> usize {
    let suffix = &text[offset.min(text.len())..];

    if suffix.starts_with("/>") {
        offset + 2
    } else if suffix.starts_with('>') {
        offset + 1
    } else {
        offset
    }
}

/// Detect whether the cursor is inside a quoted attribute value.
fn detect_attribute_value_context(tag_region: &str) -> Option<(String, String, String, usize)> {
    let tag = current_tag_name(tag_region)?;

    if let Some((attribute, fragment, fragment_start_rel)) =
        detect_quoted_attribute_value(tag_region, '"')
    {
        return Some((tag, attribute, fragment, fragment_start_rel));
    }

    if let Some((attribute, fragment, fragment_start_rel)) =
        detect_quoted_attribute_value(tag_region, '\'')
    {
        return Some((tag, attribute, fragment, fragment_start_rel));
    }

    None
}

/// Detect the current attribute and the fragment already typed in its value.
fn detect_quoted_attribute_value(tag_region: &str, quote: char) -> Option<(String, String, usize)> {
    let quote_count = tag_region.chars().filter(|c| *c == quote).count();

    // If the number of quotes is even, then the cursor is not inside an
    // unfinished quoted attribute value.
    if quote_count % 2 == 0 {
        return None;
    }

    let quote_start = tag_region.rfind(quote)?;
    let fragment_start_rel = quote_start + quote.len_utf8();
    let fragment = tag_region[fragment_start_rel..].to_string();
    let before_quote = &tag_region[..quote_start];
    let equals_index = before_quote.rfind('=')?;

    let attribute = before_quote[..equals_index]
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| is_name_char(*c))
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    if attribute.is_empty() {
        return None;
    }

    Some((attribute, fragment, fragment_start_rel))
}

/// Return the current tag name from the partial opening tag.
fn current_tag_name(tag_region: &str) -> Option<String> {
    let stripped = if let Some(stripped) = tag_region.strip_prefix("</") {
        stripped
    } else if let Some(stripped) = tag_region.strip_prefix('<') {
        stripped
    } else {
        return None;
    };

    let name: String = stripped.chars().take_while(|c| is_name_char(*c)).collect();

    if name.is_empty() { None } else { Some(name) }
}

/// Return the partially typed attribute name and the byte offset where it starts
/// inside the current tag region.
///
/// This helper is intentionally ASCII-oriented because URDF/XML attribute names
/// used in practice here are ASCII.
fn current_attribute_name_fragment(tag_region: &str) -> (String, usize) {
    let last_is_whitespace = tag_region
        .chars()
        .last()
        .map(|c| c.is_whitespace())
        .unwrap_or(false);

    if last_is_whitespace {
        return (String::new(), tag_region.len());
    }

    let bytes = tag_region.as_bytes();
    let mut start = bytes.len();

    while start > 0 {
        let ch = bytes[start - 1] as char;

        if ch.is_ascii_whitespace() || matches!(ch, '<' | '>' | '/') {
            break;
        }

        start -= 1;
    }

    let fragment = &tag_region[start..];

    if fragment.contains('=') || fragment.contains('"') || fragment.contains('\'') {
        (String::new(), tag_region.len())
    } else {
        (fragment.to_string(), start)
    }
}

/// Check whether a character is valid in an XML/URDF name.
///
/// This is intentionally conservative and focused on common URDF/XML usage.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-')
}

/// Complete known opening tag names.
fn complete_opening_tag_names(
    text: &str,
    fragment: &str,
    replace_range: ByteRange,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for tag in TAGS.iter().filter(|tag| tag.starts_with(fragment)) {
        items.extend(opening_tag_completion_items(text, tag, replace_range));
    }

    items
}

/// Complete known closing tag names.
///
/// Closing-tag completions should insert only the element name, not an entire
/// block snippet.
fn complete_closing_tag_names(
    text: &str,
    fragment: &str,
    replace_range: ByteRange,
) -> Vec<CompletionItem> {
    let range = lsp_range_from_byte_range(text, replace_range);

    TAGS.iter()
        .filter(|tag| tag.starts_with(fragment))
        .map(|tag| {
            replacement_item(
                tag,
                tag,
                CompletionItemKind::CLASS,
                "URDF closing tag",
                range,
                tag,
                Some(InsertTextFormat::PLAIN_TEXT),
                None,
            )
        })
        .collect()
}

/// Complete known attributes for a given tag.
fn complete_attribute_names(
    text: &str,
    tag: &str,
    fragment: &str,
    replace_range: ByteRange,
) -> Vec<CompletionItem> {
    let range = lsp_range_from_byte_range(text, replace_range);

    attributes_for_tag(tag)
        .iter()
        .filter(|attr| attr.starts_with(fragment))
        .map(|attr| {
            replacement_item(
                attr,
                attr,
                CompletionItemKind::FIELD,
                "URDF attribute",
                range,
                &format!(r#"{attr}="$1""#),
                Some(InsertTextFormat::SNIPPET),
                None,
            )
        })
        .collect()
}

/// Complete attribute values based on both tag and attribute.
fn complete_attribute_values(
    full_text: &str,
    tag: &str,
    attribute: &str,
    fragment: &str,
    replace_range: ByteRange,
) -> Vec<CompletionItem> {
    let range = lsp_range_from_byte_range(full_text, replace_range);

    let values: Vec<String> = match (tag, attribute) {
        ("joint", "type") => JOINT_TYPES.iter().map(|s| s.to_string()).collect(),
        ("parent", "link") | ("child", "link") => collect_named_values(full_text, "link"),
        ("material", "name") => collect_named_values(full_text, "material"),
        ("origin", "xyz") | ("axis", "xyz") => vec!["0 0 0".to_string()],
        ("origin", "rpy") => vec!["0 0 0".to_string()],
        ("box", "size") => vec!["1 1 1".to_string()],
        ("sphere", "radius") => vec!["0.1".to_string()],
        ("cylinder", "radius") => vec!["0.1".to_string()],
        ("cylinder", "length") => vec!["1.0".to_string()],
        ("mesh", "filename") => vec!["package://my_robot/meshes/part.stl".to_string()],
        ("mesh", "scale") => vec!["1 1 1".to_string()],
        ("color", "rgba") => vec!["1 1 1 1".to_string(), "0.8 0.8 0.8 1".to_string()],
        ("mass", "value") => vec!["1.0".to_string()],
        ("limit", "lower") => vec!["-1.57".to_string()],
        ("limit", "upper") => vec!["1.57".to_string()],
        ("limit", "effort") => vec!["100".to_string()],
        ("limit", "velocity") => vec!["1.0".to_string()],
        ("dynamics", "damping") => vec!["0.1".to_string()],
        ("dynamics", "friction") => vec!["0.1".to_string()],
        ("inertia", "ixx")
        | ("inertia", "ixy")
        | ("inertia", "ixz")
        | ("inertia", "iyy")
        | ("inertia", "iyz")
        | ("inertia", "izz") => vec!["0.0".to_string()],
        _ => vec![],
    };

    values
        .into_iter()
        .filter(|value| value.starts_with(fragment))
        .map(|value| {
            replacement_item(
                &value,
                &value,
                CompletionItemKind::VALUE,
                "URDF value",
                range,
                &value,
                Some(InsertTextFormat::PLAIN_TEXT),
                None,
            )
        })
        .collect()
}

/// Return completion items for an opening tag.
///
/// This function is where we decide whether a tag should offer:
/// - only a block form
/// - only a self-closing form
/// - both block and self-closing forms
fn opening_tag_completion_items(
    text: &str,
    tag: &str,
    replace_range: ByteRange,
) -> Vec<CompletionItem> {
    let range = lsp_range_from_byte_range(text, replace_range);

    match tag {
        // The root `robot` element is almost always a block.
        "robot" => vec![replacement_item(
            "robot",
            "robot",
            CompletionItemKind::CLASS,
            "URDF root element",
            range,
            "robot name=\"$1\">\n  $0\n</robot>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_robot_block".to_string()),
        )],

        // `link` can be self-closing or block-style.
        "link" => vec![
            replacement_item(
                "link",
                "link",
                CompletionItemKind::CLASS,
                "URDF link block",
                range,
                "link name=\"$1\">\n  $0\n</link>",
                Some(InsertTextFormat::SNIPPET),
                Some("01_link_block".to_string()),
            ),
            replacement_item(
                "link (self-closing)",
                "link",
                CompletionItemKind::CLASS,
                "URDF self-closing link",
                range,
                "link name=\"$1\" />",
                Some(InsertTextFormat::SNIPPET),
                Some("02_link_self".to_string()),
            ),
        ],

        // `joint` is usually block-style because it commonly contains
        // parent/child/origin/axis/limit children.
        "joint" => vec![replacement_item(
            "joint",
            "joint",
            CompletionItemKind::CLASS,
            "URDF joint block",
            range,
            "joint name=\"$1\" type=\"$2\">\n  $0\n</joint>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_joint_block".to_string()),
        )],

        // `material` may appear as a named empty element or as a block with
        // nested <color/> or <texture/>.
        "material" => vec![
            replacement_item(
                "material",
                "material",
                CompletionItemKind::CLASS,
                "URDF material block",
                range,
                "material name=\"$1\">\n  $0\n</material>",
                Some(InsertTextFormat::SNIPPET),
                Some("01_material_block".to_string()),
            ),
            replacement_item(
                "material (self-closing)",
                "material",
                CompletionItemKind::CLASS,
                "URDF self-closing material",
                range,
                "material name=\"$1\" />",
                Some(InsertTextFormat::SNIPPET),
                Some("02_material_self".to_string()),
            ),
        ],

        // Container-like tags that are normally block-style.
        "visual" => vec![replacement_item(
            "visual",
            "visual",
            CompletionItemKind::CLASS,
            "URDF visual block",
            range,
            "visual>\n  $0\n</visual>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_visual_block".to_string()),
        )],

        "collision" => vec![replacement_item(
            "collision",
            "collision",
            CompletionItemKind::CLASS,
            "URDF collision block",
            range,
            "collision>\n  $0\n</collision>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_collision_block".to_string()),
        )],

        "inertial" => vec![replacement_item(
            "inertial",
            "inertial",
            CompletionItemKind::CLASS,
            "URDF inertial block",
            range,
            "inertial>\n  $0\n</inertial>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_inertial_block".to_string()),
        )],

        "geometry" => vec![replacement_item(
            "geometry",
            "geometry",
            CompletionItemKind::CLASS,
            "URDF geometry block",
            range,
            "geometry>\n  $0\n</geometry>",
            Some(InsertTextFormat::SNIPPET),
            Some("01_geometry_block".to_string()),
        )],

        // Leaf-like tags are usually better as self-closing elements.
        "origin" => vec![replacement_item(
            "origin",
            "origin",
            CompletionItemKind::PROPERTY,
            "Pose transform",
            range,
            "origin xyz=\"$1\" rpy=\"$2\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_origin_self".to_string()),
        )],

        "parent" => vec![replacement_item(
            "parent",
            "parent",
            CompletionItemKind::PROPERTY,
            "Joint parent link",
            range,
            "parent link=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_parent_self".to_string()),
        )],

        "child" => vec![replacement_item(
            "child",
            "child",
            CompletionItemKind::PROPERTY,
            "Joint child link",
            range,
            "child link=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_child_self".to_string()),
        )],

        "axis" => vec![replacement_item(
            "axis",
            "axis",
            CompletionItemKind::PROPERTY,
            "Joint axis",
            range,
            "axis xyz=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_axis_self".to_string()),
        )],

        "limit" => vec![replacement_item(
            "limit",
            "limit",
            CompletionItemKind::PROPERTY,
            "Joint limits",
            range,
            "limit lower=\"$1\" upper=\"$2\" effort=\"$3\" velocity=\"$4\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_limit_self".to_string()),
        )],

        "dynamics" => vec![replacement_item(
            "dynamics",
            "dynamics",
            CompletionItemKind::PROPERTY,
            "Joint dynamics",
            range,
            "dynamics damping=\"$1\" friction=\"$2\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_dynamics_self".to_string()),
        )],

        "mass" => vec![replacement_item(
            "mass",
            "mass",
            CompletionItemKind::VALUE,
            "Mass value",
            range,
            "mass value=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_mass_self".to_string()),
        )],

        "inertia" => vec![replacement_item(
            "inertia",
            "inertia",
            CompletionItemKind::VALUE,
            "Inertia matrix coefficients",
            range,
            "inertia ixx=\"$1\" ixy=\"$2\" ixz=\"$3\" iyy=\"$4\" iyz=\"$5\" izz=\"$6\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_inertia_self".to_string()),
        )],

        "mesh" => vec![replacement_item(
            "mesh",
            "mesh",
            CompletionItemKind::STRUCT,
            "Mesh geometry",
            range,
            "mesh filename=\"$1\" scale=\"$2\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_mesh_self".to_string()),
        )],

        "box" => vec![replacement_item(
            "box",
            "box",
            CompletionItemKind::STRUCT,
            "Box geometry",
            range,
            "box size=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_box_self".to_string()),
        )],

        "sphere" => vec![replacement_item(
            "sphere",
            "sphere",
            CompletionItemKind::STRUCT,
            "Sphere geometry",
            range,
            "sphere radius=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_sphere_self".to_string()),
        )],

        "cylinder" => vec![replacement_item(
            "cylinder",
            "cylinder",
            CompletionItemKind::STRUCT,
            "Cylinder geometry",
            range,
            "cylinder radius=\"$1\" length=\"$2\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_cylinder_self".to_string()),
        )],

        "color" => vec![replacement_item(
            "color",
            "color",
            CompletionItemKind::VALUE,
            "Material color",
            range,
            "color rgba=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_color_self".to_string()),
        )],

        "texture" => vec![replacement_item(
            "texture",
            "texture",
            CompletionItemKind::VALUE,
            "Material texture",
            range,
            "texture filename=\"$1\" />",
            Some(InsertTextFormat::SNIPPET),
            Some("01_texture_self".to_string()),
        )],

        // Fallback: plain tag name replacement.
        _ => vec![replacement_item(
            tag,
            tag,
            CompletionItemKind::CLASS,
            "URDF tag",
            range,
            tag,
            Some(InsertTextFormat::PLAIN_TEXT),
            None,
        )],
    }
}

/// Parse all named occurrences of a tag, for example:
/// `<link name="base_link">` -> `base_link`
fn collect_named_values(text: &str, tag_name: &str) -> Vec<String> {
    let pattern = format!(
        r#"<{}\b[^>]*\bname\s*=\s*"([^"]+)""#,
        regex::escape(tag_name)
    );

    let re = Regex::new(&pattern).expect("valid regex");
    let mut set = BTreeSet::new();

    for captures in re.captures_iter(text) {
        if let Some(value) = captures.get(1) {
            set.insert(value.as_str().to_string());
        }
    }

    set.into_iter().collect()
}

/// Return the attribute set commonly used by a given URDF tag.
fn attributes_for_tag(tag: &str) -> &'static [&'static str] {
    match tag {
        "robot" => &["name"],
        "link" => &["name"],
        "joint" => &["name", "type"],
        "origin" => &["xyz", "rpy"],
        "parent" => &["link"],
        "child" => &["link"],
        "axis" => &["xyz"],
        "limit" => &["lower", "upper", "effort", "velocity"],
        "dynamics" => &["damping", "friction"],
        "material" => &["name"],
        "color" => &["rgba"],
        "texture" => &["filename"],
        "mesh" => &["filename", "scale"],
        "box" => &["size"],
        "cylinder" => &["radius", "length"],
        "sphere" => &["radius"],
        "mass" => &["value"],
        "inertia" => &["ixx", "ixy", "ixz", "iyy", "iyz", "izz"],
        _ => &[],
    }
}

/// Create a completion item that replaces an explicit LSP range.
///
/// Using `text_edit` instead of only `insert_text` is essential for correct
/// behavior when the editor has already auto-inserted delimiters like `>`
/// or `/>`.
fn replacement_item(
    label: &str,
    filter_text: &str,
    kind: CompletionItemKind,
    detail: &str,
    range: Range,
    new_text: &str,
    insert_text_format: Option<InsertTextFormat>,
    sort_text: Option<String>,
) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        filter_text: Some(filter_text.to_string()),
        sort_text,
        insert_text_format,
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: new_text.to_string(),
        })),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
