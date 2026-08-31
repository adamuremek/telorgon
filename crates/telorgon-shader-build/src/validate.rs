use spirv_tools::val::Validator;

pub fn validate(words: &[u32]) -> Result<(), String> {
    spirv_tools::val::create(Some(spirv_tools::TargetEnv::Vulkan_1_3))
        .validate(words, None)
        .map_err(|error| error.to_string())
}
