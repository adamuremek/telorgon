use rspirv::dr;
use rspirv::dr::Operand;
use rspirv::spirv::{Decoration, Op, StorageClass};

pub fn verify_interface(
    bytes: &[u8],
    expected_stage: &str,
    shader_name: &str,
) -> Result<(), String> {
    let module = dr::load_bytes(bytes).map_err(|error| error.to_string())?;
    let entry_points = module
        .entry_points
        .iter()
        .filter(|instruction| instruction.class.opcode == Op::EntryPoint)
        .count();
    if entry_points != 1 {
        return Err(format!("expected one entry point, found {entry_points}"));
    }
    if module.types_global_values.iter().any(|instruction| {
        instruction.class.opcode == Op::Variable
            && instruction.operands.iter().any(|operand| {
                matches!(
                    operand,
                    rspirv::dr::Operand::StorageClass(StorageClass::PushConstant)
                )
            })
    }) {
        return Err("push constants are forbidden by GPU ABI 1".to_owned());
    }
    if expected_stage != "vertex" && expected_stage != "fragment" {
        return Err(format!("unexpected stage {expected_stage:?}"));
    }
    let expected_instance_stride = match shader_name.split_once('_').map(|value| value.0) {
        Some("box") => 160,
        Some("glyph" | "image" | "material") => 64,
        _ => return Err(format!("unexpected shader name {shader_name:?}")),
    };
    let array_strides = module.annotations.iter().filter_map(|instruction| {
        if instruction.class.opcode != Op::Decorate {
            return None;
        }
        match instruction.operands.as_slice() {
            [
                Operand::IdRef(_),
                Operand::Decoration(Decoration::ArrayStride),
                Operand::LiteralBit32(stride),
            ] => Some(*stride),
            _ => None,
        }
    });
    if !array_strides
        .into_iter()
        .any(|stride| stride == expected_instance_stride)
    {
        return Err(format!(
            "{shader_name} does not declare its required {expected_instance_stride}-byte instance array stride"
        ));
    }
    Ok(())
}
