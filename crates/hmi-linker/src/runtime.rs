//! 汇总最终原生记录的初始文件绑定；符号路径此时已经完成重定位。
use hmi_artifact::asset::ImageInfo;
use hmi_core::{
    Result,
    controls::DATA_RECORD_LIMIT,
    dependency::RuntimeFile,
    deployment,
    diagnostic::{Diagnostic, Location},
};
use hmi_format::page::Page;
use std::collections::BTreeMap;

pub(super) fn collect(
    pages: &[Page],
    external: &[(String, ImageInfo)],
) -> Result<(Vec<serde_json::Value>, Vec<Diagnostic>)> {
    let mut files: Vec<(RuntimeFile, Vec<serde_json::Value>, bool)> = Vec::new();
    let mut indices = BTreeMap::<String, usize>::new();
    for (_, image) in external {
        let path = image.delivery.path.as_ref().unwrap();
        indices.insert(deployment::key(path), files.len());
        files.push((
            RuntimeFile::ExternalPicture { path: path.clone() },
            vec![],
            true,
        ));
    }
    let mut diagnostics = Vec::new();
    let mut count = 0;
    for (page_id, page) in pages.iter().enumerate() {
        for object in &page.objects {
            let attributes = object
                .attributes
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect::<Vec<_>>();
            let file = RuntimeFile::from_control(object.type_code, &attributes)?;
            if object.type_code == 60 && file.is_none() {
                return Err(hmi_core::error(
                    "runtime_file_path",
                    "外部图片的最终路径不能为空",
                ));
            }
            let Some(file) = file else {
                continue;
            };
            let location = Location {
                page: Some(page.name.clone()),
                object: Some(object.name.clone()),
                object_index: Some(usize::from(object.id)),
                property: Some("path".into()),
                ..Default::default()
            };
            let mut user = serde_json::json!({"page":page.name,"page_id":page_id,"object":object.name,"object_id":object.id});
            if let RuntimeFile::DataRecord {
                create_if_missing, ..
            } = &file
            {
                count += 1;
                if count > DATA_RECORD_LIMIT {
                    return Err(Diagnostic::error(
                        "control_count",
                        "一个工程最多支持八个数据记录控件",
                    )
                    .at(location)
                    .into());
                }
                user["create_if_missing"] = (*create_if_missing).into();
            }
            let path = file.path();
            let key = deployment::key(path);
            if let Some(&index) = indices.get(&key) {
                let (previous, users, _) = &mut files[index];
                match (previous, &file) {
                    (RuntimeFile::ExternalPicture { .. }, RuntimeFile::ExternalPicture { .. }) => {}
                    (
                        RuntimeFile::DataRecord { structure: old, .. },
                        RuntimeFile::DataRecord { structure, .. },
                    ) if old == structure => {
                        diagnostics.push(Diagnostic::warning("runtime_file_shared", format!(
                            "多个数据记录控件绑定文件 {path}；运行时同时打开会发生占用冲突，请在脚本中协调路径切换"
                        )).at(location));
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "runtime_file_conflict",
                            format!(
                                "运行时文件 {path} 被用于不同类型或不同结构参数，请检查绑定路径"
                            ),
                        )
                        .at(location)
                        .into());
                    }
                }
                users.push(user);
            } else {
                indices.insert(key, files.len());
                files.push((file, vec![user], false));
            }
        }
    }
    let files = files.into_iter().map(|(file, users, generated)| match file {
        RuntimeFile::DataRecord { path, structure, .. } => serde_json::json!({"kind":"data_record","path":path,"structure":structure,"generated":generated,"users":users}),
        RuntimeFile::ExternalPicture { path } => serde_json::json!({"kind":"external_picture","path":path,"generated":generated,"users":users}),
    }).collect();
    Ok((files, diagnostics))
}
