use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

// --- Status Enum ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryStatus {
    Backlog,
    Ready,
    InProgress,
    InReview,
    Done,
    Blocked,
}

impl StoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StoryStatus::Backlog => "Backlog",
            StoryStatus::Ready => "Ready",
            StoryStatus::InProgress => "In Progress",
            StoryStatus::InReview => "In Review",
            StoryStatus::Done => "Done",
            StoryStatus::Blocked => "Blocked",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "Backlog" => Ok(StoryStatus::Backlog),
            "Ready" => Ok(StoryStatus::Ready),
            "In Progress" => Ok(StoryStatus::InProgress),
            "In Review" => Ok(StoryStatus::InReview),
            "Done" => Ok(StoryStatus::Done),
            "Blocked" => Ok(StoryStatus::Blocked),
            _ => Err(format!(
                "Invalid status '{}'. Must be one of: Backlog, Ready, In Progress, In Review, Done, Blocked",
                s
            )),
        }
    }

    pub fn all_lanes() -> Vec<&'static str> {
        vec!["Backlog", "Ready", "In Progress", "In Review", "Done", "Blocked"]
    }
}

impl fmt::Display for StoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for StoryStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StoryStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        StoryStatus::from_str(&s).map_err(serde::de::Error::custom)
    }
}

// --- Data Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryTimestamps {
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryFrontmatter {
    pub id: String,
    pub epic: String,
    pub title: String,
    pub status: StoryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate_points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<HashMap<String, String>>,
    pub timestamps: StoryTimestamps,
}

#[derive(Debug, Clone)]
pub struct Story {
    pub frontmatter: StoryFrontmatter,
    pub markdown_body: String,
    pub path: String,
    pub etag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Epic {
    #[serde(rename = "epicId")]
    pub epic_id: String,
    pub slug: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryCard {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate_points: Option<f64>,
    pub tags: Vec<String>,
    pub path: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardLane {
    pub status: String,
    pub cards: Vec<StoryCard>,
}

#[derive(Debug, Clone)]
pub struct StoryIndex {
    pub stories: HashMap<String, StoryIndexEntry>,
    pub epics: Vec<Epic>,
}

#[derive(Debug, Clone)]
pub struct StoryIndexEntry {
    pub path: String,
    pub frontmatter: StoryFrontmatter,
    pub etag: String,
}

// --- Validation Types ---

#[derive(Debug, Clone, Serialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
}

// --- ETag ---

pub fn compute_etag(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// --- Parsing ---

pub fn parse_story_file(content: &str, path: &str) -> Result<Story, String> {
    // Split frontmatter from body
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(format!("Story file '{}' missing YAML frontmatter (no opening ---)", path));
    }

    // Find closing ---
    let after_first = &trimmed[3..];
    let close_pos = after_first
        .find("\n---")
        .ok_or_else(|| format!("Story file '{}' missing closing --- for frontmatter", path))?;

    let yaml_str = &after_first[..close_pos].trim();
    let body_start = close_pos + 4; // skip \n---
    let markdown_body = if body_start < after_first.len() {
        after_first[body_start..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    let frontmatter: StoryFrontmatter =
        serde_yaml::from_str(yaml_str).map_err(|e| format!("Failed to parse YAML frontmatter in '{}': {}", path, e))?;

    let etag = compute_etag(content);

    Ok(Story {
        frontmatter,
        markdown_body,
        path: path.to_string(),
        etag,
    })
}

pub fn serialize_story(fm: &StoryFrontmatter, body: &str) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    format!("---\n{}---\n{}", yaml, body)
}

// --- ID & Filename Generation ---

pub fn next_story_id(epic_id: &str, existing_ids: &[&str]) -> String {
    let epic_num = epic_id.trim_start_matches("E-");

    let mut max_seq: u32 = 0;
    let prefix = format!("S-{}-", epic_num);

    for id in existing_ids {
        if let Some(rest) = id.strip_prefix(&prefix) {
            // rest might be "01", "02", etc. — take first numeric part
            let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u32>() {
                if n > max_seq {
                    max_seq = n;
                }
            }
        }
    }

    let next = max_seq + 1;
    if next < 100 {
        format!("S-{}-{:02}", epic_num, next)
    } else {
        format!("S-{}-{}", epic_num, next)
    }
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_end_matches('-').to_string()
}

pub fn story_filename(id: &str, title: &str) -> String {
    format!("{}-{}.md", id, slugify(title))
}

// --- Scanning ---

pub fn scan_epics(base_path: &Path) -> Result<Vec<Epic>, String> {
    let mut epics = Vec::new();

    if !base_path.exists() {
        return Ok(epics);
    }

    let entries = std::fs::read_dir(base_path)
        .map_err(|e| format!("Failed to read directory '{}': {}", base_path.display(), e))?;

    let re = regex::Regex::new(r"^E-(\d+)-(.+)$").unwrap();

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(caps) = re.captures(&name) {
                let epic_num = caps.get(1).unwrap().as_str();
                let slug = caps.get(2).unwrap().as_str();
                let rel_path = base_path.join(&name);

                epics.push(Epic {
                    epic_id: format!("E-{}", epic_num),
                    slug: slug.to_string(),
                    path: rel_path.to_string_lossy().to_string(),
                });
            }
        }
    }

    epics.sort_by(|a, b| a.epic_id.cmp(&b.epic_id));
    Ok(epics)
}

pub fn scan_stories_in_epic(epic_path: &Path) -> Result<Vec<Story>, String> {
    let stories_dir = epic_path.join("stories");
    let mut stories = Vec::new();

    if !stories_dir.exists() {
        return Ok(stories);
    }

    let entries = std::fs::read_dir(&stories_dir)
        .map_err(|e| format!("Failed to read stories dir '{}': {}", stories_dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.extension().map(|e| e == "md").unwrap_or(false) {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;

            let rel_path = path.to_string_lossy().to_string();
            match parse_story_file(&content, &rel_path) {
                Ok(story) => stories.push(story),
                Err(e) => eprintln!("Warning: skipping invalid story file {}: {}", path.display(), e),
            }
        }
    }

    stories.sort_by(|a, b| a.frontmatter.id.cmp(&b.frontmatter.id));
    Ok(stories)
}

/// Find the epic folder for a given epic ID within a base directory.
/// Searches all subdirectories of notes_folder for E-<num>-* pattern.
pub fn find_epic_folder(notes_folder: &Path, epic_id: &str) -> Result<std::path::PathBuf, String> {
    let epic_num = epic_id.trim_start_matches("E-");
    let prefix = format!("E-{}-", epic_num);

    // Search recursively in notes folder for the epic dir
    fn search_dir(dir: &Path, prefix: &str) -> Option<std::path::PathBuf> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    if name.starts_with(prefix) {
                        return Some(entry.path());
                    }
                    // Recurse one level into subdirs (e.g., product/)
                    if !name.starts_with('.') {
                        if let Some(found) = search_dir(&entry.path(), prefix) {
                            return Some(found);
                        }
                    }
                }
            }
        }
        None
    }

    search_dir(notes_folder, &prefix)
        .ok_or_else(|| format!("Epic '{}' not found in '{}'", epic_id, notes_folder.display()))
}

/// Find a story file by its ID across all epics.
pub fn find_story_file(notes_folder: &Path, story_id: &str) -> Result<std::path::PathBuf, String> {
    // Extract epic number from story ID: S-0123-01 -> 0123
    let parts: Vec<&str> = story_id.split('-').collect();
    if parts.len() < 3 || parts[0] != "S" {
        return Err(format!("Invalid story ID format: '{}'. Expected S-<EPIC>-<NN>", story_id));
    }
    let epic_num = parts[1];
    let epic_id = format!("E-{}", epic_num);

    let epic_folder = find_epic_folder(notes_folder, &epic_id)?;
    let stories_dir = epic_folder.join("stories");

    if !stories_dir.exists() {
        return Err(format!("Stories directory not found for epic '{}'", epic_id));
    }

    // Find the story file starting with the story ID
    let story_prefix = format!("{}-", story_id);
    let exact_match = format!("{}.md", story_id);

    let entries = std::fs::read_dir(&stories_dir)
        .map_err(|e| format!("Failed to read stories dir: {}", e))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == exact_match || name.starts_with(&story_prefix) {
            if name.ends_with(".md") {
                return Ok(entry.path());
            }
        }
    }

    Err(format!("Story '{}' not found in '{}'", story_id, stories_dir.display()))
}

// --- Story to Card ---

pub fn story_to_card(story: &Story) -> StoryCard {
    StoryCard {
        id: story.frontmatter.id.clone(),
        title: story.frontmatter.title.clone(),
        owner: story.frontmatter.owner.clone(),
        estimate_points: story.frontmatter.estimate_points,
        tags: story.frontmatter.tags.clone().unwrap_or_default(),
        path: story.path.clone(),
        updated_at: story.frontmatter.timestamps.updated_at.clone(),
    }
}

// --- Validation ---

pub fn validate_story(story: &Story) -> (Vec<ValidationError>, Vec<ValidationWarning>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let fm = &story.frontmatter;

    // Required fields
    if fm.id.is_empty() {
        errors.push(ValidationError {
            code: "MISSING_ID".into(),
            message: "Required field 'id' is empty".into(),
        });
    }
    if fm.epic.is_empty() {
        errors.push(ValidationError {
            code: "MISSING_EPIC".into(),
            message: "Required field 'epic' is empty".into(),
        });
    }
    if fm.title.is_empty() {
        errors.push(ValidationError {
            code: "MISSING_TITLE".into(),
            message: "Required field 'title' is empty".into(),
        });
    }
    if fm.timestamps.created_at.is_empty() {
        errors.push(ValidationError {
            code: "MISSING_CREATED_AT".into(),
            message: "Required field 'timestamps.created_at' is empty".into(),
        });
    }
    if fm.timestamps.updated_at.is_empty() {
        errors.push(ValidationError {
            code: "MISSING_UPDATED_AT".into(),
            message: "Required field 'timestamps.updated_at' is empty".into(),
        });
    }

    // ID format
    let id_re = regex::Regex::new(r"^S-\d+-\d+$").unwrap();
    if !fm.id.is_empty() && !id_re.is_match(&fm.id) {
        errors.push(ValidationError {
            code: "INVALID_ID_FORMAT".into(),
            message: format!("ID '{}' does not match expected format S-<EPIC>-<NN>", fm.id),
        });
    }

    // Epic format
    let epic_re = regex::Regex::new(r"^E-\d+$").unwrap();
    if !fm.epic.is_empty() && !epic_re.is_match(&fm.epic) {
        errors.push(ValidationError {
            code: "INVALID_EPIC_FORMAT".into(),
            message: format!("Epic '{}' does not match expected format E-<NNNN>", fm.epic),
        });
    }

    // Estimate points should be non-negative
    if let Some(pts) = fm.estimate_points {
        if pts < 0.0 {
            errors.push(ValidationError {
                code: "INVALID_ESTIMATE".into(),
                message: "estimate_points must be non-negative".into(),
            });
        }
    }

    // Recommended body sections
    let recommended = ["## Problem", "## Acceptance Criteria", "## UX Notes", "## API / Data", "## Test Notes"];
    for heading in &recommended {
        if !story.markdown_body.contains(heading) {
            warnings.push(ValidationWarning {
                code: "MISSING_SECTION".into(),
                message: format!("Recommended heading '{}' missing", heading),
            });
        }
    }

    (errors, warnings)
}

// --- Audit Logging ---

pub fn append_audit_event(
    notes_folder: &Path,
    action: &str,
    id: &str,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<(), String> {
    let audit_dir = notes_folder.join(".scratch").join("audit");
    std::fs::create_dir_all(&audit_dir)
        .map_err(|e| format!("Failed to create audit directory: {}", e))?;

    let event = json!({
        "ts": now_iso8601(),
        "actor": "mcp",
        "action": action,
        "id": id,
        "before": before,
        "after": after,
    });

    let log_path = audit_dir.join("events.jsonl");
    let line = format!("{}\n", serde_json::to_string(&event).unwrap_or_default());

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open audit log: {}", e))?;

    file.write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write audit event: {}", e))?;

    Ok(())
}

// --- Helpers ---

pub fn now_iso8601() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Manual UTC formatting (avoids adding chrono dependency)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to y/m/d (simplified algorithm)
    let mut y = 1970i64;
    let mut remaining_days = days as i64;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md as i64 {
            m = i;
            break;
        }
        remaining_days -= md as i64;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Create the default template body for a new story
pub fn default_story_body() -> String {
    "## Problem\n\n\n\n## Acceptance Criteria\n\n\n\n## UX Notes\n\n\n\n## API / Data\n\n\n\n## Test Notes\n\n".to_string()
}
