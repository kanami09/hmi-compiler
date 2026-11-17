//! 从旧入口迁移的容器／页面损坏输入回归，不依赖旧工程格式。
use hmi_format::{
    binary::put_u32,
    container::{BACKUP_START, Container},
    page::Page,
    project::Project,
};
use std::collections::BTreeMap;
#[path = "support/container.rs"]
mod fixture;

#[test]
fn malformed_binary_never_panics_and_unknown_fields_roundtrip() {
    let original = fixture::fixture();
    for bytes in [
        vec![],
        original[..100].to_vec(),
        original[..original.len() - 1].to_vec(),
    ] {
        if let Ok(container) = Container::parse(bytes) {
            assert!(container.strict().is_err());
        }
    }
    let mut count = original.clone();
    put_u32(&mut count, 0, u32::MAX as usize).unwrap();
    put_u32(&mut count, BACKUP_START, u32::MAX as usize).unwrap();
    assert!(Container::parse(count).is_err());
    let mut duplicate = original.clone();
    for base in [0, BACKUP_START] {
        let index = base + 4 + 5 * 28;
        duplicate[index..index + 16].fill(0);
        duplicate[index..index + 3].copy_from_slice(b"0.i");
    }
    fixture::fix_directory(&mut duplicate);
    assert!(Container::parse(duplicate).unwrap().strict().is_err());
    let mut page = fixture::page();
    put_u32(&mut page, 56, u32::MAX as usize).unwrap();
    assert!(Page::parse(&page).is_err());
    let c = Container::parse(original.clone()).unwrap();
    assert_eq!(c.rebuild(&BTreeMap::new()).unwrap(), original);
    let p = Project::parse(&c).unwrap();
    for (_, page) in p.pages {
        for object in page.objects {
            assert_eq!(
                object.rebuild(&BTreeMap::new(), &BTreeMap::new()).unwrap(),
                object.raw
            );
        }
    }
}

#[test]
fn corrupted_page_counts_lengths_and_markers_do_not_panic() {
    let original = fixture::page();
    for offset in [4, 8, 12, 56, 60, 68, 72] {
        let mut bytes = original.clone();
        put_u32(&mut bytes, offset, u32::MAX as usize).unwrap();
        let result = std::panic::catch_unwind(|| Page::parse(&bytes));
        assert!(result.is_ok(), "偏移 {offset} 导致 panic");
    }
    for length in [0, 4, 55, 56, 90, original.len() - 1] {
        assert!(Page::parse(&original[..length]).is_err());
    }
    let object = hmi_format::page::Object::parse(&[5, 0, 0, 0, b'a', b't', b't', b'-', b'9']);
    assert!(object.is_err());
    let object = hmi_format::page::Object::parse(&[0xFF; 16]);
    assert!(object.is_err());
}
