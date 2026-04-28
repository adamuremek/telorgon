use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source_dir = manifest_dir
        .join("..")
        .join("..")
        .join("themes")
        .join("default")
        .join("materials_src");
    let output_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("out dir")).join("material_builtins");

    let compiler = find_shader_compiler().unwrap_or_else(|| {
        panic!(
            "could not find a Vulkan shader compiler; tried glslc and glslangValidator via PATH/VULKAN_SDK"
        )
    });

    std::fs::create_dir_all(&output_dir).expect("create shader output directory");

    let shaders = [
        ("blur.frag.glsl", "blur.spv"),
        ("glass.frag.glsl", "glass.spv"),
        ("shadow.frag.glsl", "shadow.spv"),
    ];

    for (source_name, output_name) in shaders {
        let source_path = source_dir.join(source_name);
        let output_path = output_dir.join(output_name);
        println!("cargo:rerun-if-changed={}", source_path.display());
        compile_fragment_shader(&compiler, &source_path, &output_path);
    }

    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
}

#[derive(Clone, Debug)]
struct ShaderCompiler {
    path: PathBuf,
    flavor: ShaderCompilerFlavor,
}

#[derive(Copy, Clone, Debug)]
enum ShaderCompilerFlavor {
    Glslc,
    GlslangValidator,
}

fn find_shader_compiler() -> Option<ShaderCompiler> {
    let mut candidates = Vec::new();

    if let Some(vulkan_sdk) = env::var_os("VULKAN_SDK") {
        let sdk_bin = PathBuf::from(vulkan_sdk).join("Bin");
        candidates.push(ShaderCompiler {
            path: sdk_bin.join(executable_name("glslc")),
            flavor: ShaderCompilerFlavor::Glslc,
        });
        candidates.push(ShaderCompiler {
            path: sdk_bin.join(executable_name("glslangValidator")),
            flavor: ShaderCompilerFlavor::GlslangValidator,
        });
    }

    candidates.push(ShaderCompiler {
        path: PathBuf::from(executable_name("glslc")),
        flavor: ShaderCompilerFlavor::Glslc,
    });
    candidates.push(ShaderCompiler {
        path: PathBuf::from(executable_name("glslangValidator")),
        flavor: ShaderCompilerFlavor::GlslangValidator,
    });

    candidates
        .into_iter()
        .find(|candidate| command_exists(&candidate.path))
}

fn compile_fragment_shader(compiler: &ShaderCompiler, source_path: &Path, output_path: &Path) {
    let mut command = Command::new(&compiler.path);
    match compiler.flavor {
        ShaderCompilerFlavor::Glslc => {
            command
                .arg("-fshader-stage=frag")
                .arg(source_path)
                .arg("-o")
                .arg(output_path);
        }
        ShaderCompilerFlavor::GlslangValidator => {
            command
                .arg("-V")
                .arg("-S")
                .arg("frag")
                .arg("-o")
                .arg(output_path)
                .arg(source_path);
        }
    }

    let output = command.output().unwrap_or_else(|error| {
        panic!(
            "failed to run shader compiler {} for {}: {error}",
            compiler.path.display(),
            source_path.display()
        )
    });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "shader compilation failed for {} with {}:\nstdout:\n{}\nstderr:\n{}",
            source_path.display(),
            compiler.path.display(),
            stdout,
            stderr
        );
    }
}

fn command_exists(path: &Path) -> bool {
    if path.components().count() > 1 {
        return path.is_file();
    }

    env::var_os("PATH")
        .map(|value| env::split_paths(&value).any(|entry| entry.join(path).is_file()))
        .unwrap_or(false)
}

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}
