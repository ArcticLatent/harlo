use crate::projects::project_rows;

/// The sidebar's rows: each project's sessions, newest first.
pub fn rebuild(rows: &mut Vec<Row>, input: &SidebarInput) {
    for project in &input.projects {
        rows.clear();
        rows.extend(project_rows(project, input));
    }
}

/// Rows matched by key: changed ones updated in place, new ones inserted.
pub fn merge_by_key(rows: &mut Vec<Row>, fresh: Vec<Row>) {
    rows.retain(|r| fresh.iter().any(|f| f.key == r.key));
    for f in fresh {
        match rows.iter_mut().find(|r| r.key == f.key) {
            Some(r) => *r = f,
            None => rows.push(f),
        }
    }
}
