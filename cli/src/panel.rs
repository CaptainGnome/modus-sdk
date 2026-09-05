use crate::i18n::LocalizedString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PANEL_JSON: &str = "assets/panel.json";
pub const PANEL_MAX_BYTES: usize = 32 * 1024;
const MAX_BLOCKS: usize = 24;
const MAX_DRAWER_CHILDREN: usize = 12;
const MAX_DRAWER_ROWS: usize = 12;
const MAX_ROW_CELLS: usize = 4;
const MAX_TRAILING: usize = 4;
const MAX_NEST_DEPTH: usize = 1;
const MAX_BUTTONS: usize = 8;
const MAX_COLUMNS: usize = 10;
const MAX_LIST_ITEM: usize = 256;
const MAX_LABEL: usize = 128;
const MAX_TABLE_ROWS: usize = 64;
const MAX_ACTIONS: usize = 8;
const MAX_ENUM_OPTIONS: usize = 32;
const MAX_HELP: usize = 256;

pub const PANEL_ICONS: &[&str] = &[
    "plus",
    "trash",
    "play",
    "pause",
    "pencil",
    "forward",
    "x-mark",
    "speaker-wave",
    "photo",
    "chevron-up",
    "chevron-down",
    "check",
    "arrow-path",
    "magnifying-glass",
    "funnel",
    "cog-6-tooth",
    "bell",
    "chat-bubble-left-right",
    "puzzle-piece",
    "currency-dollar",
    "queue-list",
    "user-group",
    "document-text",
    "eye",
    "eye-slash",
];

pub fn is_panel_icon(id: &str) -> bool {
    PANEL_ICONS.contains(&id)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Label,
    List,
    Table,
    Buttons,
    /// Collapsible section that nests other blocks (depth 1).
    Drawer,
    /// Hex color field `#RRGGBB`.
    Color,
    /// Single-choice dropdown field.
    Select,
    /// Numeric field.
    Number,
    /// Boolean toggle field.
    Toggle,
    /// String field.
    #[serde(rename = "string")]
    Text,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    #[default]
    String,
    Number,
    Bool,
    Toggle,
    Enum,
    MultiEnum,
    Media,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaAccept {
    Image,
    Audio,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PanelColumn {
    pub id: String,
    pub label: LocalizedString,
    #[serde(default, rename = "type")]
    pub column_type: ColumnType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<LocalizedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accept: Vec<MediaAccept>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelButton {
    pub id: String,
    pub label: LocalizedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelAction {
    pub id: String,
    pub label: LocalizedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PanelBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<LocalizedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<PanelColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<PanelButton>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub toolbar: Vec<PanelAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_actions: Vec<PanelAction>,
    /// Nested blocks for `drawer` (no further drawers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<PanelBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trailing: Vec<PanelBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Vec<PanelBlock>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub row_drawer: bool,
    /// Options for `select` (and fallback list when `options_source` is set).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u32>,
    /// `"system_fonts"` — FE merges OS font families into options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_source: Option<String>,
    /// Default field value (`color` / `select` / `number`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PanelSchema {
    pub version: u32,
    pub blocks: Vec<PanelBlock>,
}

impl PanelSchema {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PANEL_MAX_BYTES {
            return Err("panel.json слишком большой".into());
        }
        let schema: PanelSchema =
            serde_json::from_slice(bytes).map_err(|err| format!("panel.json: {err}"))?;
        schema.validate()?;
        Ok(schema)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 && self.version != 2 {
            return Err("panel.json: нужен version 1 или 2".into());
        }
        let mut ids = Vec::new();
        Self::validate_tree(&self.blocks, self.version, 0, &mut ids)?;
        if ids.len() > MAX_BLOCKS {
            return Err("panel.json: слишком много блоков".into());
        }
        Ok(())
    }

    fn validate_tree(
        blocks: &[PanelBlock],
        version: u32,
        depth: usize,
        ids: &mut Vec<String>,
    ) -> Result<(), String> {
        for block in blocks {
            require_key(&block.id, "id блока")?;
            if ids.iter().any(|id| id == &block.id) {
                return Err(format!("panel.json: дубль блока {}", block.id));
            }
            ids.push(block.id.clone());
            block.validate(version, depth)?;
            let nested = block.walk_nested();
            if !nested.is_empty() {
                if depth >= MAX_NEST_DEPTH {
                    return Err("panel.json: слишком глубокая вложенность".into());
                }
                for child in nested {
                    Self::validate_tree(std::slice::from_ref(child), version, depth + 1, ids)?;
                }
            }
        }
        Ok(())
    }

    pub fn i18n_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        Self::collect_i18n(&self.blocks, &mut out);
        out
    }

    fn collect_i18n(blocks: &[PanelBlock], out: &mut Vec<String>) {
        for block in blocks {
            if let Some(text) = &block.text {
                text.collect_key(out);
            }
            for column in &block.columns {
                column.label.collect_key(out);
                if let Some(help) = &column.help {
                    help.collect_key(out);
                }
            }
            for item in &block.items {
                item.label.collect_key(out);
            }
            for action in block.toolbar.iter().chain(block.row_actions.iter()) {
                action.label.collect_key(out);
            }
            for child in block.walk_nested() {
                Self::collect_i18n(std::slice::from_ref(child), out);
            }
        }
    }
}

impl PanelBlock {
    fn drawer_children(&self) -> Vec<&PanelBlock> {
        if self.rows.is_empty() {
            self.blocks.iter().collect()
        } else {
            self.rows.iter().flatten().collect()
        }
    }

    fn walk_nested(&self) -> Vec<&PanelBlock> {
        self.blocks
            .iter()
            .chain(self.rows.iter().flatten())
            .chain(self.trailing.iter())
            .collect()
    }

    fn validate(&self, version: u32, depth: usize) -> Result<(), String> {
        require_icon(self.icon.as_deref())?;
        match self.block_type {
            BlockType::Label => {
                if !self.columns.is_empty()
                    || !self.items.is_empty()
                    || !self.toolbar.is_empty()
                    || !self.row_actions.is_empty()
                    || !self.blocks.is_empty()
                    || !self.trailing.is_empty()
                    || !self.rows.is_empty()
                    || !self.layout.is_empty()
                    || self.row_drawer
                    || !self.options.is_empty()
                    || self.editable
                    || self.max_rows.is_some()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                    || self.default.is_some()
                {
                    return Err("panel.json: label без columns/items/toolbar".into());
                }
                if let Some(text) = &self.text {
                    text.validate("label text")
                        .map_err(|err| format!("panel.json: {err}"))?;
                    if text.fallback().len() > MAX_LABEL {
                        return Err("panel.json: label слишком длинный".into());
                    }
                }
            }
            BlockType::List => {
                if self.text.is_some()
                    || self.icon.is_some()
                    || !self.columns.is_empty()
                    || !self.items.is_empty()
                    || !self.toolbar.is_empty()
                    || !self.row_actions.is_empty()
                    || !self.blocks.is_empty()
                    || !self.trailing.is_empty()
                    || !self.rows.is_empty()
                    || !self.layout.is_empty()
                    || self.row_drawer
                    || !self.options.is_empty()
                    || self.editable
                    || self.max_rows.is_some()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                    || self.default.is_some()
                {
                    return Err("panel.json: list без text/columns/items".into());
                }
            }
            BlockType::Table => {
                if self.text.is_some()
                    || !self.items.is_empty()
                    || self.icon.is_some()
                    || !self.blocks.is_empty()
                    || !self.trailing.is_empty()
                    || !self.rows.is_empty()
                    || !self.options.is_empty()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                    || self.default.is_some()
                {
                    return Err("panel.json: table без text/items/icon".into());
                }
                if self.columns.is_empty() || self.columns.len() > MAX_COLUMNS {
                    return Err("panel.json: table columns".into());
                }
                if let Some(max_rows) = self.max_rows {
                    if max_rows == 0 || max_rows as usize > MAX_TABLE_ROWS {
                        return Err("panel.json: max_rows".into());
                    }
                }
                if self.editable && version < 2 {
                    return Err("panel.json: editable table нужен version 2".into());
                }
                if (!self.layout.is_empty() || self.row_drawer) && (!self.editable || version < 2) {
                    return Err(
                        "panel.json: layout/row_drawer только у editable table version 2".into(),
                    );
                }
                if !self.layout.is_empty()
                    && (self.layout.iter().any(|width| !(1..=4).contains(width))
                        || self
                            .layout
                            .iter()
                            .map(|width| *width as usize)
                            .sum::<usize>()
                            != self.columns.len())
                {
                    return Err("panel.json: table layout".into());
                }
                if (!self.toolbar.is_empty() || !self.row_actions.is_empty()) && !self.editable {
                    return Err("panel.json: toolbar/row_actions только у editable".into());
                }
                if self.toolbar.len() > MAX_ACTIONS || self.row_actions.len() > MAX_ACTIONS {
                    return Err("panel.json: слишком много actions".into());
                }
                let mut col_ids = Vec::new();
                for column in &self.columns {
                    require_key(&column.id, "id колонки")?;
                    column
                        .label
                        .validate("column label")
                        .map_err(|err| format!("panel.json: {err}"))?;
                    if col_ids.contains(&column.id) {
                        return Err(format!("panel.json: дубль колонки {}", column.id));
                    }
                    col_ids.push(column.id.clone());
                    column.validate(self.editable)?;
                }
                validate_actions(&self.toolbar, "toolbar")?;
                validate_actions(&self.row_actions, "row_actions")?;
            }
            BlockType::Buttons => {
                if self.text.is_some()
                    || !self.columns.is_empty()
                    || !self.toolbar.is_empty()
                    || !self.row_actions.is_empty()
                    || !self.blocks.is_empty()
                    || !self.trailing.is_empty()
                    || !self.rows.is_empty()
                    || !self.layout.is_empty()
                    || self.row_drawer
                    || !self.options.is_empty()
                    || self.editable
                    || self.max_rows.is_some()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                    || self.default.is_some()
                {
                    return Err("panel.json: buttons без text/columns".into());
                }
                if self.items.is_empty() || self.items.len() > MAX_BUTTONS {
                    return Err("panel.json: buttons items".into());
                }
                let mut btn_ids = Vec::new();
                for item in &self.items {
                    require_key(&item.id, "id кнопки")?;
                    item.label
                        .validate("button label")
                        .map_err(|err| format!("panel.json: {err}"))?;
                    require_icon(item.icon.as_deref())?;
                    if item.label.fallback().is_empty() && item.icon.is_none() {
                        return Err("panel.json: кнопка без label/icon".into());
                    }
                    if btn_ids.contains(&item.id) {
                        return Err(format!("panel.json: дубль кнопки {}", item.id));
                    }
                    btn_ids.push(item.id.clone());
                }
            }
            BlockType::Drawer => {
                if version < 2 {
                    return Err("panel.json: drawer нужен version 2".into());
                }
                if depth > 0 {
                    return Err("panel.json: drawer только на верхнем уровне".into());
                }
                if !self.columns.is_empty()
                    || !self.items.is_empty()
                    || !self.toolbar.is_empty()
                    || !self.row_actions.is_empty()
                    || !self.options.is_empty()
                    || self.editable
                    || self.max_rows.is_some()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || !self.layout.is_empty()
                    || self.row_drawer
                    || self.options_source.is_some()
                    || self.default.is_some()
                {
                    return Err("panel.json: drawer только text/icon/blocks".into());
                }
                if self.blocks.len() > MAX_DRAWER_CHILDREN
                    || self.rows.len() > MAX_DRAWER_ROWS
                    || self
                        .rows
                        .iter()
                        .any(|row| row.is_empty() || row.len() > MAX_ROW_CELLS)
                    || self.drawer_children().is_empty()
                {
                    return Err("panel.json: drawer blocks".into());
                }
                if self.trailing.len() > MAX_TRAILING {
                    return Err("panel.json: drawer trailing".into());
                }
                if let Some(text) = &self.text {
                    text.validate("drawer text")
                        .map_err(|err| format!("panel.json: {err}"))?;
                    if text.fallback().len() > MAX_LABEL {
                        return Err("panel.json: drawer text слишком длинный".into());
                    }
                }
                for child in self.blocks.iter().chain(self.rows.iter().flatten()) {
                    if !matches!(
                        child.block_type,
                        BlockType::Label
                            | BlockType::Color
                            | BlockType::Select
                            | BlockType::Number
                            | BlockType::Toggle
                            | BlockType::Text
                    ) {
                        return Err("panel.json: drawer только field/label".into());
                    }
                }
                for child in &self.trailing {
                    if !matches!(
                        child.block_type,
                        BlockType::Toggle | BlockType::Text | BlockType::Number
                    ) {
                        return Err(
                            "panel.json: drawer trailing только toggle/string/number".into()
                        );
                    }
                }
            }
            BlockType::Color => {
                if version < 2 {
                    return Err("panel.json: color нужен version 2".into());
                }
                self.validate_field_shell("color")?;
                if !self.options.is_empty()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                {
                    return Err("panel.json: color без options/min/max".into());
                }
                if let Some(default) = &self.default {
                    let _ = parse_hex_color(default)?;
                }
            }
            BlockType::Select => {
                if version < 2 {
                    return Err("panel.json: select нужен version 2".into());
                }
                self.validate_field_shell("select")?;
                if self.min.is_some() || self.max.is_some() {
                    return Err("panel.json: select без min/max".into());
                }
                if self.max_len.is_some() {
                    return Err("panel.json: select без max_len".into());
                }
                if self.options.is_empty() || self.options.len() > MAX_ENUM_OPTIONS {
                    return Err("panel.json: select options".into());
                }
                for opt in &self.options {
                    require_plain(opt, "select option")?;
                    if opt.is_empty() || opt.len() > MAX_LIST_ITEM {
                        return Err("panel.json: select option длина".into());
                    }
                }
                if let Some(source) = &self.options_source {
                    if source != "system_fonts" {
                        return Err("panel.json: неизвестный options_source".into());
                    }
                }
                if let Some(default) = &self.default {
                    let text = default
                        .as_str()
                        .ok_or("panel.json: select default — строка")?;
                    if self.options_source.is_none() && !self.options.iter().any(|o| o == text) {
                        return Err("panel.json: select default вне options".into());
                    }
                }
            }
            BlockType::Number => {
                if version < 2 {
                    return Err("panel.json: number block нужен version 2".into());
                }
                self.validate_field_shell("number")?;
                if !self.options.is_empty() || self.options_source.is_some() {
                    return Err("panel.json: number без options".into());
                }
                if self.max_len.is_some() {
                    return Err("panel.json: number без max_len".into());
                }
                if let (Some(min), Some(max)) = (self.min, self.max) {
                    if min > max {
                        return Err("panel.json: min > max".into());
                    }
                }
                if let Some(default) = &self.default {
                    let num = default
                        .as_f64()
                        .ok_or("panel.json: number default — число")?;
                    if let Some(min) = self.min {
                        if num < min {
                            return Err("panel.json: number default ниже min".into());
                        }
                    }
                    if let Some(max) = self.max {
                        if num > max {
                            return Err("panel.json: number default выше max".into());
                        }
                    }
                }
            }
            BlockType::Toggle => {
                if version < 2 {
                    return Err("panel.json: toggle нужен version 2".into());
                }
                self.validate_field_shell("toggle")?;
                if !self.options.is_empty()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || self.options_source.is_some()
                {
                    return Err("panel.json: toggle без options/min/max/max_len".into());
                }
                if let Some(default) = &self.default {
                    if !default.is_boolean() {
                        return Err("panel.json: toggle default — bool".into());
                    }
                }
            }
            BlockType::Text => {
                if version < 2 {
                    return Err("panel.json: string нужен version 2".into());
                }
                self.validate_field_shell("string")?;
                if !self.options.is_empty()
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.options_source.is_some()
                {
                    return Err("panel.json: string без options/min/max".into());
                }
                if matches!(self.max_len, Some(0))
                    || self.max_len.unwrap_or(1) as usize > MAX_LIST_ITEM
                {
                    return Err("panel.json: string max_len".into());
                }
                if let Some(default) = &self.default {
                    let text = default
                        .as_str()
                        .ok_or("panel.json: string default — строка")?;
                    require_plain(text, "string value")?;
                    if text.len()
                        > self
                            .max_len
                            .map(|n| n as usize)
                            .unwrap_or(MAX_LIST_ITEM)
                            .min(MAX_LIST_ITEM)
                    {
                        return Err("panel.json: string слишком длинная".into());
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_field_shell(&self, label: &str) -> Result<(), String> {
        if !self.columns.is_empty()
            || !self.items.is_empty()
            || !self.toolbar.is_empty()
            || !self.row_actions.is_empty()
            || !self.blocks.is_empty()
            || !self.trailing.is_empty()
            || !self.rows.is_empty()
            || !self.layout.is_empty()
            || self.row_drawer
            || self.editable
            || self.max_rows.is_some()
        {
            return Err(format!("panel.json: {label} без table-полей"));
        }
        if let Some(text) = &self.text {
            text.validate(&format!("{label} text"))
                .map_err(|err| format!("panel.json: {err}"))?;
            if text.fallback().len() > MAX_LABEL {
                return Err(format!("panel.json: {label} text слишком длинный"));
            }
        }
        Ok(())
    }
}

impl PanelColumn {
    fn validate(&self, editable: bool) -> Result<(), String> {
        if let Some(help) = &self.help {
            help.validate("column help")
                .map_err(|err| format!("panel.json: {err}"))?;
            if help.fallback().len() > MAX_HELP {
                return Err("panel.json: help слишком длинный".into());
            }
        }
        if !editable {
            if self.column_type != ColumnType::String
                || !self.options.is_empty()
                || !self.accept.is_empty()
                || self.unit.is_some()
            {
                return Err("panel.json: typed columns только у editable".into());
            }
            return Ok(());
        }
        match self.column_type {
            ColumnType::String => {
                if !self.options.is_empty() || !self.accept.is_empty() {
                    return Err("panel.json: string без options/accept".into());
                }
            }
            ColumnType::Number => {
                if !self.options.is_empty() || !self.accept.is_empty() || self.max_len.is_some() {
                    return Err("panel.json: number без options/accept/max_len".into());
                }
                if let (Some(min), Some(max)) = (self.min, self.max) {
                    if min > max {
                        return Err("panel.json: min > max".into());
                    }
                }
            }
            ColumnType::Bool | ColumnType::Toggle => {
                if self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || !self.options.is_empty()
                    || !self.accept.is_empty()
                {
                    return Err(format!(
                        "panel.json: {} без ограничений",
                        match self.column_type {
                            ColumnType::Toggle => "toggle",
                            _ => "bool",
                        }
                    ));
                }
            }
            ColumnType::Enum => {
                if self.options.is_empty() || self.options.len() > MAX_ENUM_OPTIONS {
                    return Err("panel.json: enum options".into());
                }
                if self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || !self.accept.is_empty()
                {
                    return Err("panel.json: enum без min/max/accept".into());
                }
                for opt in &self.options {
                    require_plain(opt, "enum option")?;
                    if opt.is_empty() || opt.len() > MAX_LIST_ITEM {
                        return Err("panel.json: enum option длина".into());
                    }
                }
            }
            ColumnType::MultiEnum => {
                if self.options.is_empty() || self.options.len() > MAX_ENUM_OPTIONS {
                    return Err("panel.json: multi_enum options".into());
                }
                if self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || !self.accept.is_empty()
                {
                    return Err("panel.json: multi_enum без min/max/accept".into());
                }
                for opt in &self.options {
                    require_plain(opt, "multi_enum option")?;
                    if opt.is_empty() || opt.len() > MAX_LIST_ITEM {
                        return Err("panel.json: multi_enum option длина".into());
                    }
                }
            }
            ColumnType::Media => {
                if self.accept.is_empty()
                    || self.accept.len() > 2
                    || self.min.is_some()
                    || self.max.is_some()
                    || self.max_len.is_some()
                    || !self.options.is_empty()
                {
                    return Err("panel.json: media.accept".into());
                }
                let mut seen = Vec::new();
                for item in &self.accept {
                    if seen.contains(item) {
                        return Err("panel.json: дубль accept".into());
                    }
                    seen.push(*item);
                }
            }
        }
        if let Some(unit) = &self.unit {
            if unit != "fx_base" {
                return Err("panel.json: неизвестный unit".into());
            }
            if self.column_type != ColumnType::Number {
                return Err("panel.json: unit только у number".into());
            }
        }
        Ok(())
    }
}

fn validate_actions(actions: &[PanelAction], label: &str) -> Result<(), String> {
    let mut ids = Vec::new();
    for action in actions {
        require_key(&action.id, &format!("id {label}"))?;
        action
            .label
            .validate(&format!("{label} label"))
            .map_err(|err| format!("panel.json: {err}"))?;
        require_icon(action.icon.as_deref())?;
        if action.label.fallback().is_empty() && action.icon.is_none() {
            return Err(format!("panel.json: {label} без label/icon"));
        }
        if ids.contains(&action.id) {
            return Err(format!("panel.json: дубль {label} {}", action.id));
        }
        ids.push(action.id.clone());
    }
    Ok(())
}

fn require_icon(icon: Option<&str>) -> Result<(), String> {
    let Some(id) = icon else {
        return Ok(());
    };
    if !is_panel_icon(id) {
        return Err(format!("panel.json: неизвестный icon {id}"));
    }
    Ok(())
}

fn require_key(raw: &str, label: &str) -> Result<(), String> {
    if !is_schema_key(raw) {
        return Err(format!("panel.json: плохой {label}"));
    }
    Ok(())
}

fn is_schema_key(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_hex_color(value: &Value) -> Result<String, String> {
    let raw = value
        .as_str()
        .ok_or_else(|| "panel: color — строка #RGB/#RRGGBB".to_string())?;
    let hex = raw.trim();
    let body = hex.strip_prefix('#').unwrap_or(hex);
    let ok = match body.len() {
        3 => body.chars().all(|c| c.is_ascii_hexdigit()),
        6 => body.chars().all(|c| c.is_ascii_hexdigit()),
        _ => false,
    };
    if !ok {
        return Err("panel: color — #RGB или #RRGGBB".into());
    }
    let full = if body.len() == 3 {
        body.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        body.to_string()
    };
    Ok(format!("#{}", full.to_ascii_lowercase()))
}

fn require_plain(raw: &str, label: &str) -> Result<(), String> {
    if raw.contains('<') || raw.contains('>') {
        return Err(format!("panel.json: HTML в {label}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_schema() -> &'static [u8] {
        r#"{
            "version": 1,
            "blocks": [
                { "id": "status", "type": "label", "text": "Очередь" },
                { "id": "queue", "type": "list" },
                { "id": "users", "type": "table", "columns": [{ "id": "name", "label": "Ник" }] },
                { "id": "bar", "type": "buttons", "items": [{ "id": "skip", "label": "Пропустить" }] }
            ]
        }"#
        .as_bytes()
    }

    #[test]
    fn parses_queue_schema() {
        let schema = PanelSchema::parse(ok_schema()).unwrap();
        assert_eq!(schema.blocks.len(), 4);
        assert_eq!(schema.blocks[0].block_type, BlockType::Label);
    }

    #[test]
    fn rejects_html_in_label() {
        let raw = br#"{
            "version": 1,
            "blocks": [{ "id": "status", "type": "label", "text": "<b>x</b>" }]
        }"#;
        let err = PanelSchema::parse(raw).unwrap_err();
        assert!(err.contains("HTML"), "{err}");
    }

    #[test]
    fn collects_i18n_keys() {
        let schema = PanelSchema::parse(
            br#"{
                "version": 1,
                "blocks": [
                    { "id": "status", "type": "label", "text": {"key": "panel.queue", "fallback": "Queue"} },
                    { "id": "bar", "type": "buttons", "items": [
                        { "id": "skip", "label": {"key": "panel.skip", "fallback": "Skip"} }
                    ]}
                ]
            }"#,
        )
        .unwrap();
        let keys = schema.i18n_keys();
        assert!(keys.contains(&"panel.queue".to_string()));
        assert!(keys.contains(&"panel.skip".to_string()));
    }

    #[test]
    fn rejects_unknown_icon() {
        let raw = br#"{
            "version": 2,
            "blocks": [{
                "id": "bar",
                "type": "buttons",
                "items": [{ "id": "skip", "label": "Skip", "icon": "not-a-real-icon" }]
            }]
        }"#;
        let err = PanelSchema::parse(raw).unwrap_err();
        assert!(err.contains("icon"), "{err}");
    }

    #[test]
    fn parses_editable_v2() {
        let raw = br#"{
            "version": 2,
            "blocks": [{
                "id": "notes",
                "type": "table",
                "editable": true,
                "columns": [
                    { "id": "title", "label": "Title", "type": "string" },
                    { "id": "done", "label": "Done", "type": "bool" }
                ],
                "toolbar": [{ "id": "add", "label": "Add", "icon": "plus" }]
            }]
        }"#;
        let schema = PanelSchema::parse(raw).unwrap();
        assert!(schema.blocks[0].editable);
    }

    #[test]
    fn parses_drawer_fields_v2() {
        let raw = br##"{
            "version": 2,
            "blocks": [{
                "id": "style",
                "type": "drawer",
                "text": "Style",
                "blocks": [
                    { "id": "fg", "type": "color", "default": "#abc" },
                    {
                        "id": "font",
                        "type": "select",
                        "options": ["Inter", "System"],
                        "options_source": "system_fonts",
                        "default": "Inter"
                    },
                    { "id": "size", "type": "number", "min": 8, "max": 72, "default": 16 }
                ]
            }]
        }"##;
        let schema = PanelSchema::parse(raw).unwrap();
        assert_eq!(schema.blocks[0].block_type, BlockType::Drawer);
        assert_eq!(schema.blocks[0].blocks.len(), 3);
    }

    #[test]
    fn rejects_nested_drawer_and_dup_ids() {
        let nested = br#"{
            "version": 2,
            "blocks": [{
                "id": "outer",
                "type": "drawer",
                "blocks": [{
                    "id": "inner",
                    "type": "drawer",
                    "blocks": [{ "id": "x", "type": "label", "text": "x" }]
                }]
            }]
        }"#;
        assert!(PanelSchema::parse(nested).is_err());

        let dup = br#"{
            "version": 2,
            "blocks": [{
                "id": "style",
                "type": "drawer",
                "blocks": [
                    { "id": "fg", "type": "color" },
                    { "id": "fg", "type": "number", "default": 1 }
                ]
            }]
        }"#;
        let err = PanelSchema::parse(dup).unwrap_err();
        assert!(err.contains("дубль"), "{err}");
    }

    #[test]
    fn mirrors_rows_trailing_toggle_string_and_table_layout() {
        let raw = br#"{
            "version": 2,
            "blocks": [
                {
                    "id": "controls",
                    "type": "drawer",
                    "rows": [[
                        { "id": "volume", "type": "number" },
                        { "id": "name", "type": "string", "max_len": 32 }
                    ]],
                    "trailing": [{ "id": "enabled", "type": "toggle", "default": true }]
                },
                {
                    "id": "entries",
                    "type": "table",
                    "editable": true,
                    "row_drawer": true,
                    "layout": [2],
                    "columns": [
                        { "id": "title", "label": "Title", "type": "string" },
                        { "id": "active", "label": "Active", "type": "bool" }
                    ]
                }
            ]
        }"#;
        let schema = PanelSchema::parse(raw).unwrap();
        assert_eq!(schema.blocks[0].rows[0][1].block_type, BlockType::Text);
        assert_eq!(schema.blocks[0].trailing[0].block_type, BlockType::Toggle);

        let bad_layout = br#"{
            "version": 2,
            "blocks": [{
                "id": "entries",
                "type": "table",
                "editable": true,
                "layout": [1],
                "columns": [
                    { "id": "title", "label": "Title" },
                    { "id": "active", "label": "Active" }
                ]
            }]
        }"#;
        assert!(PanelSchema::parse(bad_layout).is_err());
    }
}
