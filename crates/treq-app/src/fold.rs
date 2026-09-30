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

/// 折起来的一组里有几项（对象 = 几个键，数组 = 几个元素）。数不出来给 `None`。
///
/// 做法是按字节重扫这一组（开组行..结束行都算在内），只数「深度 1 上的逗号」：
/// `{`/`[` 让深度 +1，`}`/`]` 让它 -1；每对括号自己配平，所以里层的逗号碰不到深度 1。
/// 顺带就判定了类型 —— 开组行的第一个 `{`/`[` 谁先来，这组就是谁。
pub fn group_items(lines: &[String], start: usize, end: usize) -> Option<(char, usize)> {
    if end <= start {
        return None;
    }
    let mut kind: Option<char> = None;
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut children = 0usize;
    // 开组行可能带前缀（`"list": [`），组里的内容要从**开括号之后**才开始算
    let mut opened = false;
    for line in lines.get(start..=end)? {
        for tok in crate::jsonview::tokenize_line(line) {
            if !opened {
                // 还没到开括号：整行只会是空白 / 键 / 冒号之类的前缀，不算内容
                if tok.kind == crate::jsonview::TokKind::Punct
                    && let Some(open) = tok.text.bytes().find(|b| matches!(b, b'{' | b'['))
                {
                    opened = true;
                    kind = Some(open as char);
                    depth = 1;
                }
                continue;
            }
            if tok.kind != crate::jsonview::TokKind::Punct {
                continue;
            }
            for b in tok.text.bytes() {
                match b {
                    b'{' | b'[' => {
                        // 组里又开了一组 = 这个位置上有内容（哪怕它自己是空的）
                        if depth == 1 {
                            children += 1;
                        }
                        depth += 1;
                    }
                    b'}' | b']' => depth -= 1,
                    b',' if depth == 1 => commas += 1,
                    _ => {}
                }
            }
        }
    }
    // 括号没配平（流式收到一半的响应）：不报数也不猜
    let kind = kind.filter(|_| depth == 0)?;
    // 逗号 = 分项；没逗号但开括号后头有点东西（`1` / `"a": 1` / 里层又开一组）也是恰一项
    let has_content = children > 0
        || lines[start..=end].iter().skip(1).any(|l| {
            crate::jsonview::tokenize_line(l).iter().any(|t| {
                !matches!(
                    t.kind,
                    crate::jsonview::TokKind::Space | crate::jsonview::TokKind::Punct
                )
            })
        });
    let n = if commas > 0 {
        commas + 1
    } else if has_content {
        1
    } else {
        0
    };
    Some((kind, n))
}

/// 折起来那行尾巴上的「… 12 项」（`unit` 是调用方给的单位词，中文「项」/英文「items」）。
///
/// 数不出来（流式半截、空组）就只给 `…`，跟以前一样。
pub fn fold_summary(lines: &[String], start: usize, end: usize, unit: &str) -> Option<String> {
    let (_, n) = group_items(lines, start, end)?;
    Some(format!("… {n} {unit}"))
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

    #[test]
    fn counts_items_of_the_folded_group() {
        let l = lines(NESTED);
        let ends = group_ends(&l);
        // 根对象：a、d 两个键
        assert_eq!(group_items(&l, 0, ends[0].unwrap()), Some(('{', 2)));
        // "a" 对象：b、c 两个键
        assert_eq!(group_items(&l, 1, ends[1].unwrap()), Some(('{', 2)));
        // "b" 数组：1、2 两个元素（"c" 里的 "}{" 不算括号）
        assert_eq!(group_items(&l, 2, ends[2].unwrap()), Some(('[', 2)));
    }

    #[test]
    fn counts_single_item_and_empty_groups() {
        let l =
            lines("{\n  \"a\": [\n    1\n  ],\n  \"b\": {\n    \"x\": 1\n  },\n  \"c\": [\n  ]\n}");
        let ends = group_ends(&l);
        let by_line = |i: usize| group_items(&l, i, ends[i].unwrap());
        assert_eq!(by_line(1), Some(('[', 1))); // 单元素数组
        assert_eq!(by_line(4), Some(('{', 1))); // 单键对象
        assert_eq!(by_line(7), Some(('[', 0))); // 空数组
        assert_eq!(group_items(&l, 0, ends[0].unwrap()), Some(('{', 3)));
        // 整个响应就是空对象
        let l = lines("{\n}");
        let ends = group_ends(&l);
        assert_eq!(group_items(&l, 0, ends[0].unwrap()), Some(('{', 0)));
    }

    #[test]
    fn nested_commas_and_inline_children_do_not_inflate_the_count() {
        // 数组元素是行内对象 / 嵌套数组：里层的逗号不能算进来
        let l = lines("[\n  {\"a\": 1, \"b\": 2},\n  [1, 2, 3],\n  \"x\"\n]");
        let ends = group_ends(&l);
        assert_eq!(group_items(&l, 0, ends[0].unwrap()), Some(('[', 3)));
    }

    #[test]
    fn unbalanced_or_degenerate_ranges_have_no_count() {
        // 流式收到一半：括号还没配平，不报数
        let l = lines("{\n  \"a\": [\n");
        assert_eq!(group_items(&l, 0, 2), None);
        // 结束行不在开头之后（不可能折叠）也给 None
        assert_eq!(group_items(&l, 1, 1), None);
        assert_eq!(group_items(&l, 0, 99), None);
    }

    #[test]
    fn fold_summary_is_ellipsis_plus_count() {
        let l = lines(NESTED);
        let ends = group_ends(&l);
        assert_eq!(
            fold_summary(&l, 2, ends[2].unwrap(), "项").as_deref(),
            Some("… 2 项")
        );
        assert_eq!(fold_summary(&l, 3, 3, "项"), None, "不可折叠的行没有摘要");
    }
}
