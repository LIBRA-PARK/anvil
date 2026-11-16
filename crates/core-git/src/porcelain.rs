//! `git status --porcelain=v1 -z --branch` 출력 해석.

use crate::{sort_changes, ChangeKind, GitChange};

/// 브랜치 이름과 변경 목록을 뽑아낸다.
pub(crate) fn parse(output: &[u8]) -> (Option<String>, Vec<GitChange>) {
    let text = String::from_utf8_lossy(output);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut branch = None;
    let mut changes = Vec::new();

    while let Some(field) = fields.next() {
        if let Some(header) = field.strip_prefix("## ") {
            branch = parse_branch(header);
            continue;
        }
        // "XY 경로" 형식. X는 스테이징 영역, Y는 작업 폴더의 상태다.
        let (Some(code), Some(path)) = (field.get(..2), field.get(3..)) else {
            continue;
        };
        let (x, y) = (code.as_bytes()[0], code.as_bytes()[1]);
        // 이름 변경·복사는 원래 경로가 다음 필드로 따라온다.
        let old_path = if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
            fields.next().map(str::to_string)
        } else {
            None
        };
        let Some(kind) = classify(x, y) else {
            continue;
        };
        changes.push(GitChange {
            path: path.to_string(),
            // 복사는 원본이 그대로 남아 있으므로 새 파일로 본다.
            old_path: old_path.filter(|_| kind == ChangeKind::Renamed),
            kind,
        });
    }
    sort_changes(&mut changes);
    (branch, changes)
}

fn classify(x: u8, y: u8) -> Option<ChangeKind> {
    Some(match (x, y) {
        (b'?', b'?') => ChangeKind::Untracked,
        (b'!', b'!') => return None,
        (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => ChangeKind::Conflicted,
        (_, b'D') | (b'D', _) => ChangeKind::Deleted,
        (b'R', _) | (_, b'R') => ChangeKind::Renamed,
        (b'A', _) | (b'C', _) | (_, b'C') => ChangeKind::Added,
        _ => ChangeKind::Modified,
    })
}

/// `main...origin/main [ahead 1]`, `HEAD (no branch)`, `No commits yet on main` 등
fn parse_branch(header: &str) -> Option<String> {
    if header.starts_with("HEAD (no branch)") {
        return None;
    }
    let name = header
        .strip_prefix("No commits yet on ")
        .or_else(|| header.strip_prefix("Initial commit on "))
        .unwrap_or(header);
    let name = name.split("...").next().unwrap_or(name);
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Row = (String, Option<String>, ChangeKind);

    fn summary(output: &str) -> (Option<String>, Vec<Row>) {
        let (branch, changes) = parse(output.as_bytes());
        let changes = changes
            .into_iter()
            .map(|c| (c.path, c.old_path, c.kind))
            .collect();
        (branch, changes)
    }

    #[test]
    fn 상태_코드별로_분류한다() {
        let output = "## main...origin/main [ahead 1]\0 M src/lib.rs\0A  new.rs\0 D gone.rs\0?? notes/한글 메모.txt\0\
                      R  renamed.rs\0original.rs\0UU conflict.rs\0MM both.rs\0";
        let (branch, changes) = summary(output);
        assert_eq!(branch.as_deref(), Some("main"));
        use ChangeKind::*;
        assert_eq!(
            changes,
            vec![
                ("both.rs".into(), None, Modified),
                ("conflict.rs".into(), None, Conflicted),
                ("gone.rs".into(), None, Deleted),
                ("new.rs".into(), None, Added),
                ("notes/한글 메모.txt".into(), None, Untracked),
                ("renamed.rs".into(), Some("original.rs".into()), Renamed),
                ("src/lib.rs".into(), None, Modified),
            ]
        );
    }

    #[test]
    fn 브랜치_표기를_해석한다() {
        assert_eq!(summary("## feature/x\0").0.as_deref(), Some("feature/x"));
        assert_eq!(summary("## HEAD (no branch)\0").0, None);
        assert_eq!(
            summary("## No commits yet on main\0").0.as_deref(),
            Some("main")
        );
        assert_eq!(summary("").0, None);
    }
}
