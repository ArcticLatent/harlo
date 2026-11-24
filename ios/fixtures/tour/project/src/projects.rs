/// A project's sessions as sidebar rows, and the list kept in step.
use crate::sidebar::Row;

/// The rows for one project's sessions, newest first.
pub fn project_rows(project: &Project, input: &SidebarInput) -> Vec<Row> {
    let mut sessions: Vec<&Session> = input.sessions.iter().filter(|s| s.project == project.id).collect();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.updated));
    sessions.into_iter().map(Row::from).collect()
}









/// What the list shows when a session changes.
pub fn on_session_changed(rows: &mut Vec<Row>, input: &SidebarInput) {
    // A changed status, a new title or a new session: all of them
    // come here, many times a minute while agents work.
















/// Called for every change.
pub fn refresh(rows: &mut Vec<Row>, input: &SidebarInput) {
    rows.clear(); // every change rebuilds the list
    for project in &input.projects {
        rows.extend(project_rows(project, input));
    }
}
