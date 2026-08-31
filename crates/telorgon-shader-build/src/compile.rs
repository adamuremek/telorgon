use shaderc::{CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion};

pub fn compile(source: &str, source_name: &str, stage: &str) -> Result<Vec<u32>, String> {
    let compiler = Compiler::new().map_err(|error| error.to_string())?;
    let mut options = CompileOptions::new().map_err(|error| error.to_string())?;
    options.set_target_env(shaderc::TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
    options.set_target_spirv(SpirvVersion::V1_6);
    options.set_optimization_level(OptimizationLevel::Performance);
    options.set_warnings_as_errors();
    let kind = match stage {
        "vertex" => ShaderKind::Vertex,
        "fragment" => ShaderKind::Fragment,
        other => return Err(format!("unsupported shader stage {other:?}")),
    };
    compiler
        .compile_into_spirv(source, kind, source_name, "main", Some(&options))
        .map(|artifact| artifact.as_binary().to_vec())
        .map_err(|error| error.to_string())
}
