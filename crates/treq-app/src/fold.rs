//! 响应正文的 JSON 折叠：把「开组行 → 它的结束行」配好对，再按折叠集合算出可见行。
//!
//! 纯逻辑（不 use gpui），所以可以直接写 `#[test]`；界面只负责画箭头和存折叠集合。

use std::collections::HashSet;

/// 每行如果是一组 `{`/`[` 的开头，配到它的结束行下标。
///
/// 单行自带开闭（`{}`、`"a": [1, 2]`）的行配不到（返回 `None`），
/// 这种行折叠起来没有意义，界面也就不画箭头。
/// 括号只认 JSON 词法里的标点 —— 字符串里的 `{` 不算（交给 `jsonview::tokenize_line`）。
pub fn group_ends(lines: &[String]) -> Vec<Option<usize>> {
    let mut ends = vec![None; lines.len()];
    // 栈里只存「是哪一行的括号」，同行的开闭自己配平，不跨行就不记
    let mut stack: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for tok in crate::jsonview::tokenize_line(line) {
            // tokenizer 会把 `{"a":` 这类连成一个 Punct，所以按字节逐个认括号
            if tok.kind != crate::jsonview::TokKind::Punct {
                continue;
            }
            for b in tok.text.bytes() {
                match b {
                    b'{' | b'[' => stack.push(i),
                    b'}' | b']' => {
                        if let Some(open_line) = stack.pop()
                            && open_line != i
                        {
                            ends[open_line] = Some(i);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    ends
}

/// 依据折叠集合算可见行（被折起来的那段整段跳过）。
pub fn visible_indices(ends: &[Option<usize>], folded: &HashSet<usize>) -> Vec<usize> {
    let mut out = Vec::with_capacity(ends.len());
    let mut i = 0;
    while i < ends.len() {
        out.push(i);
        match (folded.contains(&i), ends[i]) {
            (true, Some(end)) if end > i => i = end + 1,
            _ => i += 1,
        }
    }
    out
}

/// 「折叠全部」：只折当前能看见的那一层（最外层各段）。
///
/// 里层不塞进集合 —— 这样把外层展开时看到的就是完整内容，而不是一堆还是折着的节点。
pub fn fold_all(ends: &[Option<usize>]) -> HashSet<usize> {
    let mut set = HashSet::new();
    let mut i = 0;
    while i < ends.len() {
        match ends[i] {
            Some(end) if end > i => {
                set.insert(i);
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    set
}

/// 有东西可以折吗（界面据此决定要不要给「折叠全部」按钮）。
pub fn has_foldable(ends: &[Option<usize>]) -> bool {
    ends.iter()
        .enumerate()
        .any(|(i, e)| matches!(e, Some(end) if *end > i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(|l| l.to_string()).collect()
    }

    const NESTED: &str = r#"{
  "a": {
    "b": [
      1,
      2
    ],
    "c": "}{"
  },
  "d": {}
}"#;

    #[test]
    fn pairs_open_and_close_lines() {
        let l = lines(NESTED);
        let ends = group_ends(&l);
        assert_eq!(ends[0], Some(9)); // 根对象
        assert_eq!(ends[1], Some(7)); // "a": {
        assert_eq!(ends[2], Some(5)); // "b": [
        assert_eq!(ends[3], None); // 1,
        assert_eq!(ends[6], None); // "c": "}{" —— 字符串里的括号不算
        assert_eq!(ends[8], None); // "d": {} —— 单行自带开闭
    }

    #[test]
    fn folds_hide_the_matched_range() {
        let l = lines(NESTED);
        let ends = group_ends(&l);
        let folded: HashSet<usize> = [2].into_iter().collect(); // 折 "b": [
        assert_eq!(visible_indices(&ends, &folded), vec![0, 1, 2, 6, 7, 8, 9]);
        let all: HashSet<usize> = [0].into_iter().collect(); // 折根
        assert_eq!(visible_indices(&ends, &all), vec![0]);
    }

    #[test]
    fn fold_all_only_folds_the_outermost_level() {
        let l = lines(NESTED);
        let ends = group_ends(&l);
        let set = fold_all(&ends);
        assert_eq!(set, [0].into_iter().collect::<HashSet<usize>>());
        assert_eq!(visible_indices(&ends, &set).len(), 1);
        // 展开根之后，里面还是完整的（里层没被偷偷折上）
        assert_eq!(visible_indices(&ends, &HashSet::new()).len(), l.len());
        assert!(has_foldable(&ends));
    }

    #[test]
    fn arrays_of_inline_objects_dont_confuse_the_pairs() {
        let l = lines("[\n  {\"a\": 1},\n  {\"b\": 2}\n]");
        let ends = group_ends(&l);
        assert_eq!(ends[0], Some(3)); // 数组
        assert_eq!(ends[1], None); // 行内对象自己配平了
        assert_eq!(ends[2], None);
        assert_eq!(visible_indices(&ends, &[0].into_iter().collect()), vec![0]);
    }

    #[test]
    fn empty_structures_and_plain_text_have_nothing_to_fold() {
        let l = lines("{\n}");
        assert_eq!(group_ends(&l)[0], Some(1));
        assert!(has_foldable(&group_ends(&l)));
        let plain = lines("hello\nworld");
        assert!(!has_foldable(&group_ends(&plain)));
        assert_eq!(
            visible_indices(&group_ends(&plain), &HashSet::new()),
            vec![0, 1]
        );
    }

    #[test]
    fn unmatched_brackets_do_not_panic() {
        let l = lines("{\n  \"a\": [\n");
        let ends = group_ends(&l);
        assert_eq!(ends[0], None);
        assert_eq!(ends[1], None);
        assert!(!has_foldable(&ends));
    }

    #[test]
    fn stale_fold_indices_are_ignored() {
        let l = lines("{\n  \"a\": 1\n}");
        let ends = group_ends(&l);
        // 折一个不存在 / 不可折的下标，不该丢掉任何行
        let folded: HashSet<usize> = [99, 1].into_iter().collect();
        assert_eq!(visible_indices(&ends, &folded), vec![0, 1, 2]);
    }
}
