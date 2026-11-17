// 最开始是使用 build.rs 脚本生成 protobuf 对应的 `.rs` 文件。
// 由于不方便查看，所以直接将输出目录中的文件复制到项目中来了，所以这个脚本可以先注释了

fn main() -> std::io::Result<()> {
    // plain buffer

    // let mut config = prost_build::Config::new();
    // config.out_dir("src/protos").extern_path(".table_store", "crate::protos");
    // config.compile_protos(
    //     &[
    //         "src/protos/table_store.proto",
    //         "src/protos/table_store_filter.proto",
    //         "src/protos/table_store_search.proto",
    //         "src/protos/timeseries.proto",
    //     ],
    //     &["src/protos/"],
    // )?;

    Ok(())
}
