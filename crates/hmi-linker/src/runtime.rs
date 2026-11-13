//! 汇总初始运行时文件绑定，不读取或修改本机、虚拟 SD 或设备上的文件。
use hmi_artifact::CompiledObject;
use hmi_core::{
    Result,
    controls::DATA_RECORD_LIMIT,
    dependency::RuntimeFile,
    diagnostic::{Diagnostic, Location},
};
use std::collections::BTreeMap;

pub(super) fn collect(
    pages: &[CompiledObject],
) -> Result<(Vec<serde_json::Value>, Vec<Diagnostic>)> {
    let mut files: Vec<(RuntimeFile, Vec<serde_json::Value>)> = Vec::new();
    let mut indices = BTreeMap::<String, usize>::new();
    let mut diagnostics = Vec::new();
    let mut count = 0;
    for (page_id, page) in pages.iter().enumerate() {
        let page_name = page.metadata.name.as_ref().unwrap();
        let objects = &page.metadata.layout.as_ref().unwrap().objects;
        for dependency in &page.metadata.runtime_dependencies {
            let object = &objects[dependency.object];
            let location = Location {
                page: Some(page_name.clone()),
                object: Some(object.name.clone()),
                object_index: Some(dependency.object),
                property: Some("path".into()),
                ..Default::default()
            };
            count += 1;
            if count > DATA_RECORD_LIMIT {
                return Err(
                    Diagnostic::error("control_count", "一个工程最多支持八个数据记录控件")
                        .at(location)
                        .into(),
                );
            }
            let RuntimeFile::DataRecord {
                path,
                structure,
                create_if_missing,
            } = &dependency.file;
            // SD 卡文件名按 ASCII 大小写不敏感检查；不改写记录中的原始路径。
            let key = if path
                .get(..4)
                .is_some_and(|s| s.eq_ignore_ascii_case("sd0/"))
            {
                path.to_ascii_lowercase()
            } else {
                path.clone()
            };
            let user = serde_json::json!({"page":page_name,"page_id":page_id,
                "object":object.name,"object_id":dependency.object,"create_if_missing":create_if_missing});
            if let Some(&index) = indices.get(&key) {
                let (previous, users) = &mut files[index];
                let RuntimeFile::DataRecord { structure: old, .. } = previous;
                if old != structure {
                    return Err(Diagnostic::error("runtime_file_conflict", format!(
                        "数据记录 {}.{} 与 {}.{} 绑定同一文件 {path}，但 lenth、maxval 或 dez 不一致",
                        users[0]["page"].as_str().unwrap(), users[0]["object"].as_str().unwrap(), page_name, object.name
                    )).at(location).into());
                }
                diagnostics.push(Diagnostic::warning("runtime_file_shared", format!(
                    "多个数据记录控件绑定文件 {path}；运行时同时打开会发生占用冲突，请在脚本中协调路径切换"
                )).at(location));
                users.push(user);
            } else {
                indices.insert(key, files.len());
                files.push((dependency.file.clone(), vec![user]));
            }
        }
    }
    let files = files
        .into_iter()
        .map(|(file, users)| {
            let RuntimeFile::DataRecord {
                path, structure, ..
            } = file;
            serde_json::json!({"kind":"data_record","path":path,"structure":structure,
            "generated":false,"users":users})
        })
        .collect();
    Ok((files, diagnostics))
}
