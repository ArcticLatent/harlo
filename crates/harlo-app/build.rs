use std::path::{Path, PathBuf};

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

/// The bundled Geist and JetBrains Mono weights (also listed in src/fonts.rs).
const FONTS: [&str; 8] = ["Regular", "Italic", "Medium", "MediumItalic", "SemiBold", "SemiBoldItalic", "Bold", "BoldItalic"];

fn main() {
    println!("cargo::rerun-if-changed=fonts");
    // File-type icons: bundled as resources, and their ids listed for
    // src/file_icons.rs.
    let mut icons: Vec<String> = std::fs::read_dir("icons/file-types")
        .expect("icons/file-types")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".svg"))
        .collect();
    icons.sort();
    // Sorted by id (not file name: "go-mod.svg" < "go.svg" but "go" < "go-mod").
    let mut ids: Vec<&str> = icons.iter().map(|n| n.trim_end_matches(".svg")).collect();
    ids.sort();
    let ids: Vec<String> = ids.iter().map(|id| format!("{id:?}")).collect();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("file_icons.rs");
    std::fs::write(out, format!("pub const ICONS: &[&str] = &[{}];\n", ids.join(", "))).unwrap();
    println!("cargo::rerun-if-changed=icons/file-types");
    // Provider logos, shown next to the model.
    let mut providers: Vec<String> = std::fs::read_dir("icons/providers")
        .expect("icons/providers")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".svg"))
        .collect();
    providers.sort();
    println!("cargo::rerun-if-changed=icons/providers");
    // The logos' SVG text, for recolouring at run time (src/provider_logos.rs).
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let logos: String = providers
        .iter()
        .map(|n| format!("    (\"{}\", include_str!(\"{}/icons/providers/{}\")),\n", n.trim_end_matches(".svg"), manifest, n))
        .collect();
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("provider_logos.rs"),
        format!("pub const LOGOS: &[(&str, &str)] = &[\n{logos}];\n"),
    )
    .unwrap();

    // Colour themes, compiled in for src/themes.rs.
    let mut themes: Vec<PathBuf> = std::fs::read_dir("themes")
        .expect("themes")
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    themes.sort();
    let items: Vec<String> = themes
        .iter()
        .map(|p| format!("include_str!({:?})", std::fs::canonicalize(p).unwrap()))
        .collect();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("themes.rs");
    std::fs::write(out, format!("pub const THEME_FILES: &[&str] = &[{}];\n", items.join(", "))).unwrap();
    println!("cargo::rerun-if-changed=themes");
    println!("cargo::rerun-if-changed=cpp");

    CxxQtBuilder::new_qml_module(QmlModule::new("harlo")
            .qml_file(QmlFile::from("qml/Theme.qml").singleton(true))
            .qml_file(QmlFile::from("qml/Keymap.qml").singleton(true))
            .qml_files([
                "qml/Main.qml",
                "qml/Sidebar.qml",
                "qml/ChatView.qml",
                "qml/Composer.qml",
                "qml/ChangesPanel.qml",
                "qml/WorktreeDialogs.qml",
                "qml/TabStrip.qml",
                "qml/CommandPalette.qml",
                "qml/RightRail.qml",
                "qml/LeftRail.qml",
                "qml/MiniWindow.qml",
                "qml/ThemeOrb.qml",
                "qml/GitHostCard.qml",
                "qml/SetupGuide.qml",
                "qml/PullRequestDialog.qml",
                "qml/UndoDialog.qml",
                "qml/UserAvatar.qml",
                "qml/AvatarPicker.qml",
                "qml/ContextPanel.qml",
                "qml/ActivityPanel.qml",
                "qml/FilesPanel.qml",
                "qml/FileEditor.qml",
                "qml/EditorHighlighter.qml",
                "qml/TranscriptView.qml",
                "qml/PromptRail.qml",
                "qml/BranchPicker.qml",
                "qml/StashesDialog.qml",
                "qml/GoalDock.qml",
                "qml/GoalPopover.qml",
                "qml/SubagentRow.qml",
                "qml/MiniChat.qml",
                "qml/NotesPanel.qml",
                "qml/PlanLimits.qml",
                "qml/ProviderLogo.qml",
                "qml/FlatButton.qml",
                "qml/Chip.qml",
                "qml/CountBadge.qml",
                "qml/StatusDot.qml",
                "qml/StatsView.qml",
                "qml/SettingsDialog.qml",
                "qml/AppSwitch.qml",
                "qml/ShortcutList.qml",
                "qml/KeyCap.qml",
                "qml/AskPanel.qml",
                "qml/DraftView.qml",
                "qml/TerminalView.qml",
                "qml/TerminalPanel.qml",
                "qml/SubagentDock.qml",
                "qml/SubagentDialog.qml",
                "qml/TodoDock.qml",
                "qml/BtwPopover.qml",
                "qml/DelegatePopover.qml",
                "qml/DiffView.qml",
                "qml/UpdateToasts.qml",
                "qml/PhoneFirewallToast.qml",
                "qml/AddHostDialog.qml",
                "qml/InstanceSwitcher.qml",
                "qml/SearchDialog.qml",
                "qml/BranchDialog.qml",
                "qml/TrayIcon.qml",
                "qml/DropDownContent.qml",
                "qml/DropDownLayer.qml",
                "qml/DropDownPlain.qml",
                "qml/RunView.qml",
                "qml/ShortcutsDialog.qml",
                "qml/Icon.qml",
                "qml/ComboField.qml",
                "qml/FileIcon.qml",
                "qml/AppMenu.qml",
                "qml/AppMenuItem.qml",
                "qml/AppMenuSeparator.qml",
                "qml/AppDialog.qml",
                "qml/RemoteFolderDialog.qml",
                "qml/McpPage.qml",
                "qml/PairDeviceDialog.qml",
                "qml/AgentAvatar.qml",
                "qml/PanelHeader.qml",
                "qml/PanelButton.qml",
                "qml/SignInDialog.qml",
                "qml/GlassSurface.qml",
                "qml/GlassScrim.qml",
                "qml/AppSlider.qml",
                "qml/ComposerIconButton.qml",
                "qml/ComposerLabelButton.qml",
                "qml/ControlSurface.qml",
                "qml/LogoMark.qml",
                "qml/ProjectIcon.qml",
                "qml/ProviderMark.qml",
                "qml/AppButtonBox.qml",
                "qml/DialogButton.qml",
                "qml/AppTextField.qml",
                "qml/AppCheckBox.qml",
                "qml/AttachmentChip.qml",
            ]))
        .qrc_resources(
            icons
                .iter()
                .map(|n| format!("icons/file-types/{n}"))
                .chain(["harlo", "tray", "tray-busy", "tray-input"].map(|n| format!("icons/app/{n}.svg")))
                .chain(providers.iter().map(|n| format!("icons/providers/{n}")))
                // Your avatar (the left rail, your turns in a chat).
                .chain(["beanie", "classic", "glasses", "headphones", "jacket", "scarf"].map(|n| format!("icons/avatars/penguin-{n}.png")))
                // The interface (Geist) and code (JetBrains Mono) fonts, bundled
                // (SIL OFL, fonts/Geist-OFL.txt, fonts/OFL.txt): the same
                // whether or not they're installed.
                .chain(FONTS.iter().flat_map(|w| [format!("fonts/Geist-{w}.ttf"), format!("fonts/JetBrainsMono-{w}.ttf")]))
                .collect::<Vec<_>>(),
        )
        .include_dir("cpp")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("Widgets")
        .qt_module("Test")
        .qt_module("Multimedia")
        .files(["src/qt_base.rs", "src/chat_session.rs", "src/workspace.rs", "src/git_panel.rs", "src/file_browser.rs", "src/text_file.rs", "src/usage_stats.rs", "src/fonts.rs", "src/terminal_session.rs", "src/platform.rs", "src/test_input.rs", "src/dictation.rs", "src/audio.rs", "src/remotes.rs", "src/updates.rs", "src/mcp.rs", "src/phone.rs", "src/accounts.rs", "src/setup.rs"])
        .build();
}
