//! Normalized UI element roles.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Normalized cross-platform UI roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Top-level application window.
    Window,
    /// Modal or modeless dialog window.
    Dialog,
    /// Alert or urgent notification popup.
    Alert,
    /// Menu bar containing top-level menus.
    MenuBar,
    /// Pop-up menu or dropdown menu.
    Menu,
    /// Individual item inside a menu.
    MenuItem,
    /// Toolbar containing action controls.
    ToolBar,
    /// Clickable push button.
    Button,
    /// Button paired with a secondary dropdown action.
    SplitButton,
    /// Hyperlink navigation element.
    Link,
    /// Container for tab buttons.
    TabList,
    /// Tab navigation header element.
    Tab,
    /// Panel displayed when a tab is active.
    TabPanel,
    /// Single-line editable text input.
    TextField,
    /// Multi-line editable text input.
    TextArea,
    /// Dedicated search input field.
    SearchField,
    /// Masked password input field.
    PasswordField,
    /// Dropdown combo box selection control.
    ComboBox,
    /// List box selection container.
    ListBox,
    /// Selectable item inside a list or combo box.
    Option,
    /// Two- or three-state checkbox control.
    CheckBox,
    /// Mutually exclusive radio selection button.
    RadioButton,
    /// Binary switch / toggle toggle control.
    Switch,
    /// Numeric range slider control.
    Slider,
    /// Numeric increment/decrement spin button.
    SpinButton,
    /// Hierarchical tree view container.
    Tree,
    /// Node inside a hierarchical tree view.
    TreeItem,
    /// Structured data table container.
    Table,
    /// Interactive spreadsheet or data grid.
    Grid,
    /// Single row in a table or grid.
    Row,
    /// Header cell for a column.
    ColumnHeader,
    /// Header cell for a row.
    RowHeader,
    /// Individual data cell in a table or grid.
    Cell,
    /// Generic list container.
    List,
    /// Item inside a generic list.
    ListItem,
    /// Section or content heading.
    Heading,
    /// Static text label.
    Label,
    /// Static or interactive image / icon.
    Image,
    /// Progress bar or loading indicator.
    Progress,
    /// Status bar or transient status notification.
    Status,
    /// Help tooltip overlay.
    Tooltip,
    /// Logical group or container for related controls.
    Group,
    /// Structural landmark region (banner, main, navigation, etc.).
    Region,
    /// Scrollable content viewport.
    ScrollArea,
    /// Generic read-only document surface.
    Document,
    /// Browser web page rendering area.
    WebArea,
    /// Custom-drawn graphic canvas.
    Canvas,
    /// Element with an unknown or unrecognized role.
    #[default]
    Unknown,
}

impl Role {
    /// Returns the canonical string representation for this role.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Dialog => "dialog",
            Self::Alert => "alert",
            Self::MenuBar => "menu_bar",
            Self::Menu => "menu",
            Self::MenuItem => "menu_item",
            Self::ToolBar => "tool_bar",
            Self::Button => "button",
            Self::SplitButton => "split_button",
            Self::Link => "link",
            Self::TabList => "tab_list",
            Self::Tab => "tab",
            Self::TabPanel => "tab_panel",
            Self::TextField => "text_field",
            Self::TextArea => "text_area",
            Self::SearchField => "search_field",
            Self::PasswordField => "password_field",
            Self::ComboBox => "combo_box",
            Self::ListBox => "list_box",
            Self::Option => "option",
            Self::CheckBox => "check_box",
            Self::RadioButton => "radio_button",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::SpinButton => "spin_button",
            Self::Tree => "tree",
            Self::TreeItem => "tree_item",
            Self::Table => "table",
            Self::Grid => "grid",
            Self::Row => "row",
            Self::ColumnHeader => "column_header",
            Self::RowHeader => "row_header",
            Self::Cell => "cell",
            Self::List => "list",
            Self::ListItem => "list_item",
            Self::Heading => "heading",
            Self::Label => "label",
            Self::Image => "image",
            Self::Progress => "progress",
            Self::Status => "status",
            Self::Tooltip => "tooltip",
            Self::Group => "group",
            Self::Region => "region",
            Self::ScrollArea => "scroll_area",
            Self::Document => "document",
            Self::WebArea => "web_area",
            Self::Canvas => "canvas",
            Self::Unknown => "unknown",
        }
    }

    /// Returns true if this role represents an interactive control (clickable, selectable, editable).
    #[must_use]
    pub const fn is_interactive(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::SplitButton
                | Self::Link
                | Self::Tab
                | Self::MenuItem
                | Self::TextField
                | Self::TextArea
                | Self::SearchField
                | Self::PasswordField
                | Self::ComboBox
                | Self::ListBox
                | Self::Option
                | Self::CheckBox
                | Self::RadioButton
                | Self::Switch
                | Self::Slider
                | Self::SpinButton
                | Self::TreeItem
                | Self::Row
                | Self::Cell
                | Self::ListItem
        )
    }

    /// Returns true if this role represents a structural landmark or salient container.
    #[must_use]
    pub const fn is_landmark(self) -> bool {
        matches!(
            self,
            Self::Window
                | Self::Dialog
                | Self::Alert
                | Self::MenuBar
                | Self::ToolBar
                | Self::TabList
                | Self::Tab
                | Self::Heading
                | Self::Region
        )
    }

    /// Returns true if this role represents a container of other elements.
    #[must_use]
    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::Window
                | Self::Dialog
                | Self::Alert
                | Self::MenuBar
                | Self::Menu
                | Self::ToolBar
                | Self::TabList
                | Self::TabPanel
                | Self::ListBox
                | Self::Tree
                | Self::TreeItem
                | Self::Table
                | Self::Grid
                | Self::Row
                | Self::List
                | Self::Group
                | Self::Region
                | Self::ScrollArea
                | Self::Document
                | Self::WebArea
        )
    }

    /// Returns true if this role represents an editable text entry field.
    #[must_use]
    pub const fn is_text_input(self) -> bool {
        matches!(
            self,
            Self::TextField
                | Self::TextArea
                | Self::SearchField
                | Self::PasswordField
                | Self::ComboBox
                | Self::SpinButton
        )
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Role {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        let role = match normalized.as_str() {
            "window" => Self::Window,
            "dialog" => Self::Dialog,
            "alert" => Self::Alert,
            "menu_bar" | "menubar" => Self::MenuBar,
            "menu" => Self::Menu,
            "menu_item" | "menuitem" => Self::MenuItem,
            "tool_bar" | "toolbar" => Self::ToolBar,
            "button" => Self::Button,
            "split_button" | "splitbutton" => Self::SplitButton,
            "link" => Self::Link,
            "tab_list" | "tablist" => Self::TabList,
            "tab" => Self::Tab,
            "tab_panel" | "tabpanel" => Self::TabPanel,
            "text_field" | "textfield" | "edit" | "input" => Self::TextField,
            "text_area" | "textarea" => Self::TextArea,
            "search_field" | "searchfield" | "search" => Self::SearchField,
            "password_field" | "password" => Self::PasswordField,
            "combo_box" | "combobox" => Self::ComboBox,
            "list_box" | "listbox" => Self::ListBox,
            "option" => Self::Option,
            "check_box" | "checkbox" => Self::CheckBox,
            "radio_button" | "radio" => Self::RadioButton,
            "switch" => Self::Switch,
            "slider" => Self::Slider,
            "spin_button" | "spinbutton" => Self::SpinButton,
            "tree" => Self::Tree,
            "tree_item" | "treeitem" => Self::TreeItem,
            "table" => Self::Table,
            "grid" => Self::Grid,
            "row" => Self::Row,
            "column_header" | "columnheader" => Self::ColumnHeader,
            "row_header" | "rowheader" => Self::RowHeader,
            "cell" => Self::Cell,
            "list" => Self::List,
            "list_item" | "listitem" => Self::ListItem,
            "heading" => Self::Heading,
            "label" | "text" | "static_text" => Self::Label,
            "image" | "graphic" | "icon" => Self::Image,
            "progress" | "progress_bar" => Self::Progress,
            "status" | "status_bar" => Self::Status,
            "tooltip" => Self::Tooltip,
            "group" => Self::Group,
            "region" | "landmark" => Self::Region,
            "scroll_area" | "scrollpane" => Self::ScrollArea,
            "document" => Self::Document,
            "web_area" | "webarea" => Self::WebArea,
            "canvas" => Self::Canvas,
            _ => Self::Unknown,
        };
        Ok(role)
    }
}
