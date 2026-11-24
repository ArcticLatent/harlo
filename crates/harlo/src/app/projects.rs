//! Settings → Projects: each project's name, colour, icon and worktree
//! command (kept on the server, for every client), the model its new
//! sessions start with (kept in Harlo), its worktrees, and removing it from
//! Harlo.

// Rust guideline compliant 2026-10-09

use harlo_core::server::{PROJECT_COLORS, ProjectChange};
use harlo_core::workspace::Fetch;

use super::sheets::{Event as SheetEvent, Section, Sheet};
use super::{App, Message};
use crate::model::ModelChoice;
use crate::task::Task;

/// The side, in pixels, a project's icon image is kept at.
const ICON_SIDE: u32 = 64;

/// Something done in Settings → Projects.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectEvent {
    /// Show this project's settings, by its folder, or the list of projects.
    Pick(Option<String>),
    /// Open Settings at this project's settings, by its folder.
    Open(String),
    /// Give the shown project colour `.0`, one of [`PROJECT_COLORS`], or
    /// none.
    Color(Option<usize>),
    /// Choose an image file for the shown project's icon.
    ChooseIcon,
    /// The image chosen, as a `data:` URL, or why it couldn't be read;
    /// `None` when nothing was chosen.
    IconRead(Option<Result<String, String>>),
    /// Go back to the plain mark.
    ClearIcon,
    /// Start new sessions in the shown project with this model, or with
    /// OpenCode's default.
    Model(Option<ModelChoice>),
    /// A change to the project was saved, or why it couldn't be.
    Saved(Result<(), String>),
    /// Remove one of the shown project's worktrees.
    RemoveWorktree {
        /// The worktree's folder.
        directory: String,
        /// Whether to remove it though it has changes.
        force: bool,
    },
    /// A worktree was removed, or why not, and whether forcing would do.
    WorktreeRemoved(String, Result<(), (String, bool)>),
    /// A worktree was made, in this folder, or why not.
    WorktreeMade(Result<String, String>),
    /// Archive the shown project's sessions when removing it, or keep them.
    Archive(bool),
    /// Remove the shown project from Harlo.
    Hide,
    /// Archiving its sessions finished, or why some couldn't be.
    Archived(Result<(), String>),
}

/// Settings → Projects' state.
#[derive(Debug, Default)]
pub struct Projects {
    /// The folder of the project shown, if one is.
    pub shown: Option<String>,
    /// Why the last change couldn't be made, if it couldn't.
    pub error: Option<String>,
    /// A worktree that has changes, by its folder, which removing needs
    /// forcing.
    pub needs_force: Option<String>,
    /// Whether a worktree is being made.
    pub making: bool,
    /// Whether removing a project archives its sessions.
    pub archive: bool,
}

impl App {
    /// Applies something done in Settings → Projects.
    pub(super) fn project_event(&mut self, event: ProjectEvent) -> Task<Message> {
        match event {
            ProjectEvent::Pick(folder) => self.show_project(folder),
            ProjectEvent::Open(folder) => {
                // Already open, it stays open, at the project.
                let task = if self.sheets.open == Some(Sheet::Settings) {
                    self.sheet_event(SheetEvent::Section(Section::Projects))
                } else {
                    self.sheet_event(SheetEvent::Open(Section::Projects))
                };
                self.show_project(Some(folder));
                return task;
            }
            ProjectEvent::Color(color) => {
                let color = color
                    .and_then(|i| PROJECT_COLORS.get(i))
                    .map_or_else(String::new, |c| (*c).to_owned());
                return self.change_project(ProjectChange {
                    color: Some(color),
                    ..ProjectChange::default()
                });
            }
            ProjectEvent::ChooseIcon => {
                return Task::perform(choose_icon(), |read| {
                    Message::Sheet(SheetEvent::Project(ProjectEvent::IconRead(read)))
                });
            }
            ProjectEvent::IconRead(None) => {}
            ProjectEvent::IconRead(Some(Err(why))) => self.sheets.projects.error = Some(why),
            ProjectEvent::IconRead(Some(Ok(url))) => {
                return self.change_project(ProjectChange {
                    icon: Some(url),
                    ..ProjectChange::default()
                });
            }
            ProjectEvent::ClearIcon => {
                return self.change_project(ProjectChange {
                    icon: Some(String::new()),
                    ..ProjectChange::default()
                });
            }
            ProjectEvent::Model(model) => return self.set_project_model(model),
            ProjectEvent::Saved(result) | ProjectEvent::Archived(result) => {
                self.sheets.projects.error = result.err();
            }
            ProjectEvent::RemoveWorktree { directory, force } => {
                return self.remove_worktree(directory, force);
            }
            ProjectEvent::WorktreeRemoved(directory, result) => {
                return self.worktree_removed(&directory, result);
            }
            ProjectEvent::WorktreeMade(result) => {
                self.sheets.projects.making = false;
                match result {
                    Ok(_) => return self.refetch_worktrees(),
                    Err(why) => self.sheets.projects.error = Some(why),
                }
            }
            ProjectEvent::Archive(archive) => self.sheets.projects.archive = archive,
            ProjectEvent::Hide => return self.hide_project(),
        }
        Task::none()
    }

    /// Shows `folder`'s settings, or the list.
    fn show_project(&mut self, folder: Option<String>) {
        self.sheets.projects.shown = folder;
        self.sheets.projects.error = None;
        self.sheets.projects.needs_force = None;
        self.sheets.projects.archive = false;
    }

    /// The server's ID for the project shown, if it has one.
    fn shown_project_id(&self) -> Option<String> {
        let folder = self.sheets.projects.shown.as_deref()?;
        Some(self.workspace.project(folder)?.id.clone())
    }

    /// Saves `change` to the shown project, on the server.
    fn change_project(&mut self, change: ProjectChange) -> Task<Message> {
        let (Some(client), Some(id)) = (self.client.clone(), self.shown_project_id()) else {
            self.sheets.projects.error =
                Some("OpenCode doesn't know this project until it has a session.".to_owned());
            return Task::none();
        };
        self.sheets.projects.error = None;
        // The server announces the change, which reloads the projects.
        Task::perform(
            async move {
                client
                    .update_project(&id, &change)
                    .await
                    .map(|_| ())
                    .map_err(|e| format!("Couldn't save it: {e}"))
            },
            |result| Message::Sheet(SheetEvent::Project(ProjectEvent::Saved(result))),
        )
    }

    /// Saves the shown project's name, or its worktree command, as typed.
    pub(super) fn set_project_text(&mut self, name: bool, value: &str) -> Task<Message> {
        let value = value.trim().to_owned();
        let change = if name {
            ProjectChange {
                name: Some(value),
                ..ProjectChange::default()
            }
        } else {
            ProjectChange {
                start: Some(value),
                ..ProjectChange::default()
            }
        };
        self.change_project(change)
    }

    /// Sets the model new sessions in the shown project start with.
    fn set_project_model(&mut self, model: Option<ModelChoice>) -> Task<Message> {
        let Some(folder) = self.sheets.projects.shown.clone() else {
            return Task::none();
        };
        match model {
            Some(model) => {
                let id = format!("{}/{}", model.provider_id, model.id);
                self.preferences.project_models.insert(folder, id);
            }
            None => {
                self.preferences.project_models.remove(&folder);
            }
        }
        self.save_preferences()
    }

    /// Makes a worktree of the shown project, named `name`, or named by
    /// OpenCode when it's empty.
    pub(super) fn make_worktree(&mut self, name: &str) -> Task<Message> {
        let (Some(client), Some(id)) = (self.client.clone(), self.shown_project_id()) else {
            return Task::none();
        };
        let name = name.trim().to_owned();
        self.sheets.projects.error = None;
        self.sheets.projects.making = true;
        Task::perform(
            async move {
                let name = (!name.is_empty()).then_some(name.as_str());
                client
                    .create_worktree(&id, name, None)
                    .await
                    .map_err(|e| format!("Couldn't make it: {e}"))
            },
            |result| Message::Sheet(SheetEvent::Project(ProjectEvent::WorktreeMade(result))),
        )
    }

    /// Removes the shown project's worktree in `directory`.
    fn remove_worktree(&mut self, directory: String, force: bool) -> Task<Message> {
        let (Some(client), Some(id)) = (self.client.clone(), self.shown_project_id()) else {
            return Task::none();
        };
        self.sheets.projects.error = None;
        self.sheets.projects.needs_force = None;
        let folder = directory.clone();
        Task::perform(
            async move {
                client
                    .remove_worktree(&id, &folder, force)
                    .await
                    .map_err(|e| (e.to_string(), e.needs_force()))
            },
            move |result| {
                Message::Sheet(SheetEvent::Project(ProjectEvent::WorktreeRemoved(
                    directory, result,
                )))
            },
        )
    }

    /// Follows a worktree's removal: reloads the list, or offers to force it.
    fn worktree_removed(
        &mut self,
        directory: &str,
        result: Result<(), (String, bool)>,
    ) -> Task<Message> {
        match result {
            Ok(()) => return self.refetch_worktrees(),
            Err((_, true)) => self.sheets.projects.needs_force = Some(directory.to_owned()),
            Err((why, false)) => {
                self.sheets.projects.error = Some(format!("Couldn't remove it: {why}"));
            }
        }
        Task::none()
    }

    /// Reloads the shown project's worktrees, though the server also says
    /// when they change.
    fn refetch_worktrees(&self) -> Task<Message> {
        self.shown_project_id()
            .map_or_else(Task::none, |id| self.fetch(vec![Fetch::Worktrees(id)]))
    }

    /// Takes the shown project out of the sidebar, archiving its sessions
    /// when asked; its folder and files stay.
    fn hide_project(&mut self) -> Task<Message> {
        let Some(folder) = self.sheets.projects.shown.take() else {
            return Task::none();
        };
        let archive: Vec<String> = if self.sheets.projects.archive {
            self.projects
                .iter()
                .filter(|p| p.directory == folder)
                .flat_map(|p| {
                    p.sessions
                        .iter()
                        .chain(p.worktrees.iter().flat_map(|w| &w.sessions))
                })
                .map(|s| s.id.clone())
                .collect()
        } else {
            Vec::new()
        };
        self.workspace.hide_project(&folder);
        self.preferences.project_models.remove(&folder);
        self.show_project(None);
        self.refresh();
        let saved = self.save_preferences();
        let Some(client) = self.client.clone().filter(|_| !archive.is_empty()) else {
            return saved;
        };
        let archived = Task::perform(
            async move {
                let mut failed = 0;
                for id in &archive {
                    if client.set_archived(id, true).await.is_err() {
                        failed += 1;
                    }
                }
                if failed == 0 {
                    Ok(())
                } else {
                    Err(format!("{failed} of its sessions couldn't be archived."))
                }
            },
            |result| Message::Sheet(SheetEvent::Project(ProjectEvent::Archived(result))),
        );
        Task::batch([saved, archived])
    }

    /// What the server keeps for the project in `folder`: its name, colour,
    /// icon and worktree command; `None` before its first session.
    #[must_use]
    pub fn server_project(&self, folder: &str) -> Option<&harlo_core::workspace::ServerProject> {
        self.workspace.project(folder)
    }

    /// The project folders removed from the sidebar.
    #[must_use]
    pub fn hidden_projects(&self) -> &[String] {
        self.workspace.hidden()
    }

    /// The model chosen for new sessions in the project shown in Settings,
    /// if one was.
    #[must_use]
    pub fn shown_project_model(&self) -> Option<ModelChoice> {
        let folder = self.sheets.projects.shown.as_deref()?;
        let model = self.project_model(folder)?;
        let name = self
            .catalogs
            .values()
            .find_map(|c| c.model(&model.provider_id, &model.id))
            .map_or_else(|| model.id.clone(), |m| m.name.clone());
        Some(ModelChoice {
            provider_id: model.provider_id,
            id: model.id,
            name,
        })
    }

    /// The name of the worktree session `id` runs in; `None` when it runs
    /// in a project's own folder, or isn't known.
    #[must_use]
    pub fn worktree_of(&self, id: &str) -> Option<&str> {
        let folder = &self.workspace.session(id)?.directory;
        self.projects
            .iter()
            .flat_map(|p| &p.worktrees)
            .find(|w| &w.directory == folder)
            .map(|w| w.name.as_str())
    }

    /// The model new sessions in `folder`, or a worktree of its project,
    /// start with, when one was chosen for the project.
    pub(super) fn project_model(&self, folder: &str) -> Option<harlo_core::workspace::ModelRef> {
        let project = self
            .projects
            .iter()
            .find(|p| p.directory == folder || p.worktrees.iter().any(|w| w.directory == folder))?;
        let chosen = self.preferences.project_models.get(&project.directory)?;
        let (provider, model) = chosen.split_once('/')?;
        Some(harlo_core::workspace::ModelRef {
            provider_id: provider.to_owned(),
            id: model.to_owned(),
            variant: None,
        })
    }
}

/// Asks for an image file and reads it as a small PNG `data:` URL;
/// `None` when none was chosen.
async fn choose_icon() -> Option<Result<String, String>> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Project icon")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "svg"])
        .pick_file()
        .await?;
    let path = file.path().to_path_buf();
    Some(
        tokio::task::spawn_blocking(move || icon_url(&path))
            .await
            .unwrap_or_else(|e| Err(e.to_string())),
    )
}

/// The image at `path`, fitted in a square of [`ICON_SIDE`] pixels, as a
/// PNG `data:` URL.
///
/// # Errors
///
/// Fails when the file can't be read or isn't an image.
pub fn icon_url(path: &std::path::Path) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read it: {e}"))?;
    let svg = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    let picture = if svg {
        svg_image(&bytes)?
    } else {
        image::load_from_memory(&bytes)
            .map_err(|e| format!("That isn't an image Harlo can read: {e}"))?
    };
    let fitted = picture.resize(ICON_SIDE, ICON_SIDE, image::imageops::FilterType::Lanczos3);
    let mut png = std::io::Cursor::new(Vec::new());
    fitted
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| format!("Couldn't convert it: {e}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
    Ok(format!("data:image/png;base64,{encoded}"))
}

/// An SVG drawn at [`ICON_SIDE`] pixels.
fn svg_image(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map_err(|e| format!("That isn't an SVG Harlo can read: {e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIDE, ICON_SIDE)
        .ok_or_else(|| "Couldn't draw it.".to_owned())?;
    let drawn = tree.size();
    let pixels = f32::from(u16::try_from(ICON_SIDE).unwrap_or(u16::MAX));
    let scale = pixels / drawn.width().max(drawn.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // Premultiplied to straight alpha.
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(ICON_SIDE, ICON_SIDE, pixels)
        .map(image::DynamicImage::ImageRgba8)
        .ok_or_else(|| "Couldn't draw it.".to_owned())
}
