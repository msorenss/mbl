//! Native HailoRT 4.x (Hailo-8 / Hailo-8L) and HailoRT 5.x (Hailo-10H) runtime driver.
//!
//! Provides three cooperative execution tiers (`group_id = "SHARED"`, `ROUND_ROBIN` scheduling,
//! and background scheduler priority `8` < normal `16` so real-time NVR co-tenants such as Frigate
//! are never starved):
//! 1. **Tier 1 (`InferModel` Native Bridge):** Unified HailoRT C++ `VDevice` + `InferModel` +
//!    `ConfiguredInferModel` pipeline supporting both Hailo-8/8L and Hailo-10H at 100+ FPS with
//!    4096-byte page-aligned DMA buffers.
//! 2. **Tier 2 (Pure C `libhailort.so` VStreams FFI via `dlopen`):** Direct C ABI fallback for
//!    Hailo-8 / Hailo-8L hosts without C++ headers installed, plus universal `hailo_identify` and
//!    `hailo_get_chip_temperature` hardware probing on all Hailo architectures.
//! 3. **Tier 3 (`hailortcli` CLI Runner):** Subprocess fallback when only `/usr/bin/hailortcli`
//!    is present.

use std::ffi::{CStr, CString, c_char, c_int, c_uchar, c_uint, c_void};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use image::DynamicImage;
use image::imageops::FilterType;
use mbl_hwaccel::fingerprint::detect_fingerprint_from_root;
use mbl_hwaccel::hailo::{HailoConfig, HailoSharingMode, discover_hef_model_for_profile};

/// Standard 80 COCO object detection classes used by Hailo Model Zoo YOLOv6/v8/v11 `.hef` models.
pub const COCO_CLASSES: [&str; 80] = [
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "backpack",
    "umbrella",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "dining table",
    "toilet",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
];

/// Single bounding-box detection produced by a Hailo NPU NMS output stream.
#[derive(Debug, Clone, PartialEq)]
pub struct CocoDetection {
    pub class_id: usize,
    pub label: &'static str,
    pub score: f32,
    /// Normalized `[y_min, x_min, y_max, x_max]` in `0.0..=1.0`.
    pub bbox: [f32; 4],
}

/// Output from executing a single frame on the Hailo-8 / Hailo-8L / Hailo-10H NPU.
#[derive(Debug, Clone)]
pub struct NpuFrameResult {
    pub backend_name: String,
    pub model_name: String,
    pub detections: Vec<CocoDetection>,
    pub dense_features: Vec<f32>,
    pub chip_temp_c: Option<f32>,
}

/// Single transcribed speech segment returned by Hailo-10H `hailort::genai::Speech2Text` (`Whisper`).
#[derive(Debug, Clone, PartialEq)]
pub struct HailoSpeechSegment {
    pub start_sec: f32,
    pub end_sec: f32,
    pub text: String,
}

/// Live device identification queried directly from `libhailort.so`.
#[derive(Debug, Clone, PartialEq)]
pub struct LibHailoProbe {
    pub library_version: String,
    pub firmware_version: String,
    pub architecture: String,
    pub chip_temperature_c: Option<f32>,
}

#[cfg(unix)]
#[allow(unsafe_code)]
mod dl {
    use super::{c_char, c_int, c_void};

    pub const RTLD_NOW: c_int = 2;
    pub const RTLD_LOCAL: c_int = 0;

    unsafe extern "C" {
        pub fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
}

/// C++ `InferModel` bridge source compiled on-demand when `/usr/include/hailo/hailort.hpp` and
/// `libhailort.so` are installed on the host. Supports both Hailo-8/8L and Hailo-10H.
#[cfg(unix)]
const INFER_BRIDGE_CPP_SOURCE: &str = r#"
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <memory>
#include <string>
#include <hailo/hailort.hpp>

using namespace hailort;

struct MblHailoSession {
    std::unique_ptr<VDevice> vdevice;
    std::shared_ptr<InferModel> infer_model;
    ConfiguredInferModel configured;
    ConfiguredInferModel::Bindings bindings;
    void *in_buf = nullptr;
    void *out_buf = nullptr;
    size_t in_size = 0;
    size_t out_size = 0;
    uint32_t width = 0;
    uint32_t height = 0;
    uint32_t channels = 0;
    uint32_t num_classes = 0;
    int is_nms = 0;

    ~MblHailoSession() {
        if (in_buf) std::free(in_buf);
        if (out_buf) std::free(out_buf);
    }
};

extern "C" {

MblHailoSession* mbl_hailo_session_create(
    const char *hef_path,
    const char *group_id,
    uint8_t scheduler_priority,
    uint32_t *out_w,
    uint32_t *out_h,
    uint32_t *out_c,
    size_t *out_in_bytes,
    size_t *out_out_bytes,
    uint32_t *out_num_classes,
    int *out_is_nms
) {
    if (!hef_path || !group_id) return nullptr;

    hailo_vdevice_params_t params;
    if (hailo_init_vdevice_params(&params) != HAILO_SUCCESS) return nullptr;
    params.group_id = group_id;
    params.scheduling_algorithm = HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN;

    auto vdevice_exp = VDevice::create(params);
    if (!vdevice_exp) return nullptr;
    auto vdevice = vdevice_exp.release();

    auto infer_model_exp = vdevice->create_infer_model(std::string(hef_path));
    if (!infer_model_exp) return nullptr;
    auto infer_model = infer_model_exp.release();

    auto input_exp = infer_model->input();
    auto output_exp = infer_model->output();
    if (!input_exp || !output_exp) return nullptr;

    input_exp->set_format_type(HAILO_FORMAT_TYPE_UINT8);
    output_exp->set_format_type(HAILO_FORMAT_TYPE_FLOAT32);

    auto shape = input_exp->shape();
    bool is_nms = output_exp->is_nms();
    uint32_t num_classes = 0;
    if (is_nms) {
        auto nms_exp = output_exp->get_nms_shape();
        if (nms_exp) {
            num_classes = nms_exp->number_of_classes;
        }
    }

    auto configured_exp = infer_model->configure();
    if (!configured_exp) return nullptr;
    auto configured = configured_exp.release();
    configured.set_scheduler_priority(scheduler_priority);

    auto bindings_exp = configured.create_bindings();
    if (!bindings_exp) return nullptr;
    auto bindings = bindings_exp.release();

    size_t in_size = input_exp->get_frame_size();
    size_t out_size = output_exp->get_frame_size();
    if (in_size == 0 || out_size == 0) return nullptr;

    size_t in_alloc = (in_size + 4095ULL) & ~4095ULL;
    size_t out_alloc = (out_size + 4095ULL) & ~4095ULL;
    void *in_buf = aligned_alloc(4096, in_alloc);
    void *out_buf = aligned_alloc(4096, out_alloc);
    if (!in_buf || !out_buf) {
        if (in_buf) std::free(in_buf);
        if (out_buf) std::free(out_buf);
        return nullptr;
    }
    std::memset(in_buf, 0, in_alloc);
    std::memset(out_buf, 0, out_alloc);

    if (bindings.input()->set_buffer(MemoryView(in_buf, in_size)) != HAILO_SUCCESS) {
        std::free(in_buf);
        std::free(out_buf);
        return nullptr;
    }
    if (bindings.output()->set_buffer(MemoryView(out_buf, out_size)) != HAILO_SUCCESS) {
        std::free(in_buf);
        std::free(out_buf);
        return nullptr;
    }

    auto *sess = new MblHailoSession{
        std::move(vdevice),
        std::move(infer_model),
        std::move(configured),
        std::move(bindings),
        in_buf,
        out_buf,
        in_size,
        out_size,
        shape.width,
        shape.height,
        shape.features,
        num_classes,
        is_nms ? 1 : 0
    };

    if (out_w) *out_w = sess->width;
    if (out_h) *out_h = sess->height;
    if (out_c) *out_c = sess->channels;
    if (out_in_bytes) *out_in_bytes = sess->in_size;
    if (out_out_bytes) *out_out_bytes = sess->out_size;
    if (out_num_classes) *out_num_classes = sess->num_classes;
    if (out_is_nms) *out_is_nms = sess->is_nms;

    return sess;
}

int mbl_hailo_session_infer(
    MblHailoSession *sess,
    const uint8_t *rgb_data,
    size_t rgb_len,
    float *out_floats,
    size_t out_bytes,
    uint32_t timeout_ms
) {
    if (!sess || !rgb_data || !out_floats) return -1;
    size_t copy_in = rgb_len < sess->in_size ? rgb_len : sess->in_size;
    std::memcpy(sess->in_buf, rgb_data, copy_in);
    std::memset(sess->out_buf, 0, sess->out_size);

    hailo_status st = sess->configured.run(sess->bindings, std::chrono::milliseconds(timeout_ms));
    if (st != HAILO_SUCCESS) {
        return static_cast<int>(st);
    }

    size_t copy_out = out_bytes < sess->out_size ? out_bytes : sess->out_size;
    std::memcpy(out_floats, sess->out_buf, copy_out);
    return 0;
}

int mbl_hailo_session_temperature(MblHailoSession *sess, float *out_temp_c) {
    if (!sess || !out_temp_c) return -1;
    auto phys_exp = sess->vdevice->get_physical_devices();
    if (!phys_exp || phys_exp->empty()) return -1;
    Device &dev = phys_exp->at(0).get();
    auto temp_exp = dev.get_chip_temperature();
    if (temp_exp && temp_exp->ts0_temperature > -40.0f) {
        *out_temp_c = temp_exp->ts0_temperature;
        return 0;
    }
    auto health_exp = dev.query_health_stats();
    if (health_exp && health_exp->on_die_temperature > -40.0f) {
        *out_temp_c = health_exp->on_die_temperature;
        return 0;
    }
    return -1;
}

void mbl_hailo_session_destroy(MblHailoSession *sess) {
    if (sess) {
        delete sess;
    }
}

static std::string json_escape(const char *s) {
    if (!s) return "";
    std::string out;
    for (const char *p = s; *p != '\0'; ++p) {
        char c = *p;
        switch (c) {
            case '\"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default: out += c; break;
        }
    }
    return out;
}

int mbl_hailo_genai_vlm_frame_shape(
    const char *hef_path,
    const char *group_id,
    uint32_t *out_w,
    uint32_t *out_h,
    uint32_t *out_c
) {
    if (!hef_path || !group_id) return -1;
    hailo_vdevice_params_t params;
    if (hailo_init_vdevice_params(&params) != HAILO_SUCCESS) return -1;
    params.group_id = group_id;
    params.scheduling_algorithm = HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN;

    auto vdevice_exp = VDevice::create_shared(params);
    if (!vdevice_exp) return -1;
    auto vdevice = vdevice_exp.release();

    genai::VLMParams vlm_params(std::string(hef_path), true);
    auto vlm_exp = genai::VLM::create(vdevice, vlm_params);
    if (!vlm_exp) return -1;
    auto vlm = vlm_exp.release();

    auto shape = vlm.input_frame_shape();
    if (out_w) *out_w = shape.width;
    if (out_h) *out_h = shape.height;
    if (out_c) *out_c = shape.features;
    return 0;
}

int mbl_hailo_genai_vlm_describe(
    const char *hef_path,
    const char *group_id,
    const char *prompt,
    const uint8_t *rgb_data,
    size_t rgb_len,
    uint32_t max_tokens,
    char *out_text,
    size_t out_capacity
) {
    if (!hef_path || !group_id || !prompt || !rgb_data || !out_text || out_capacity == 0) return -1;
    out_text[0] = '\0';

    hailo_vdevice_params_t params;
    if (hailo_init_vdevice_params(&params) != HAILO_SUCCESS) return -1;
    params.group_id = group_id;
    params.scheduling_algorithm = HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN;

    auto vdevice_exp = VDevice::create_shared(params);
    if (!vdevice_exp) return -2;
    auto vdevice = vdevice_exp.release();

    genai::VLMParams vlm_params(std::string(hef_path), true);
    auto vlm_exp = genai::VLM::create(vdevice, vlm_params);
    if (!vlm_exp) return -3;
    auto vlm = vlm_exp.release();

    uint32_t expected_sz = vlm.input_frame_size();
    if (expected_sz == 0 || rgb_len < expected_sz) return -4;

    auto gen_params_exp = vlm.create_generator_params();
    if (!gen_params_exp) return -5;
    auto gen_params = gen_params_exp.release();
    if (max_tokens > 0) {
        gen_params.set_max_generated_tokens(max_tokens);
    }

    std::string msg = "{\"role\":\"user\",\"content\":[{\"type\":\"image\"},{\"type\":\"text\",\"text\":\"" + json_escape(prompt) + "\"}]}";
    std::vector<std::string> messages = { msg };
    std::vector<MemoryView> frames = { MemoryView(const_cast<uint8_t*>(rgb_data), expected_sz) };

    auto completion_exp = vlm.generate(gen_params, messages, frames);
    if (!completion_exp) return -6;
    auto completion = completion_exp.release();

    auto text_exp = completion.read_all(std::chrono::seconds(25));
    if (!text_exp) return -7;
    std::string result = text_exp.release();

    size_t copy_len = result.size() < (out_capacity - 1) ? result.size() : (out_capacity - 1);
    std::memcpy(out_text, result.data(), copy_len);
    out_text[copy_len] = '\0';
    return static_cast<int>(copy_len);
}

int mbl_hailo_genai_llm_generate(
    const char *hef_path,
    const char *group_id,
    const char *prompt,
    uint32_t max_tokens,
    char *out_text,
    size_t out_capacity
) {
    if (!hef_path || !group_id || !prompt || !out_text || out_capacity == 0) return -1;
    out_text[0] = '\0';

    hailo_vdevice_params_t params;
    if (hailo_init_vdevice_params(&params) != HAILO_SUCCESS) return -1;
    params.group_id = group_id;
    params.scheduling_algorithm = HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN;

    auto vdevice_exp = VDevice::create_shared(params);
    if (!vdevice_exp) return -2;
    auto vdevice = vdevice_exp.release();

    genai::LLMParams llm_params(std::string(hef_path), "", true);
    auto llm_exp = genai::LLM::create(vdevice, llm_params);
    if (!llm_exp) return -3;
    auto llm = llm_exp.release();

    auto gen_params_exp = llm.create_generator_params();
    if (!gen_params_exp) return -4;
    auto gen_params = gen_params_exp.release();
    if (max_tokens > 0) {
        gen_params.set_max_generated_tokens(max_tokens);
    }

    std::string msg = "{\"role\":\"user\",\"content\":\"" + json_escape(prompt) + "\"}";
    std::vector<std::string> messages = { msg };

    auto completion_exp = llm.generate(gen_params, messages);
    if (!completion_exp) return -5;
    auto completion = completion_exp.release();

    auto text_exp = completion.read_all(std::chrono::seconds(25));
    if (!text_exp) return -6;
    std::string result = text_exp.release();

    size_t copy_len = result.size() < (out_capacity - 1) ? result.size() : (out_capacity - 1);
    std::memcpy(out_text, result.data(), copy_len);
    out_text[copy_len] = '\0';
    return static_cast<int>(copy_len);
}

int mbl_hailo_genai_speech2text(
    const char *hef_path,
    const char *group_id,
    const float *pcm_f32_16k,
    size_t num_samples,
    const char *language,
    int translate_to_english,
    char *out_json,
    size_t out_capacity
) {
    if (!hef_path || !group_id || !pcm_f32_16k || num_samples == 0 || !out_json || out_capacity == 0) return -1;
    out_json[0] = '\0';

    hailo_vdevice_params_t params;
    if (hailo_init_vdevice_params(&params) != HAILO_SUCCESS) return -1;
    params.group_id = group_id;
    params.scheduling_algorithm = HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN;

    auto vdevice_exp = VDevice::create_shared(params);
    if (!vdevice_exp) return -2;
    auto vdevice = vdevice_exp.release();

    genai::Speech2TextParams s2t_params(hef_path);
    auto s2t_exp = genai::Speech2Text::create(vdevice, s2t_params);
    if (!s2t_exp) return -3;
    auto s2t = s2t_exp.release();

    auto gen_params_exp = s2t.create_generator_params();
    if (!gen_params_exp) return -4;
    auto gen_params = gen_params_exp.release();
    if (translate_to_english) {
        gen_params.set_task(genai::Speech2TextTask::TRANSLATE);
    } else {
        gen_params.set_task(genai::Speech2TextTask::TRANSCRIBE);
    }
    if (language && language[0] != '\0') {
        gen_params.set_language(language);
    }

    MemoryView audio_view(const_cast<float*>(pcm_f32_16k), num_samples * sizeof(float));
    auto segs_exp = s2t.generate_all_segments(audio_view, gen_params, std::chrono::seconds(60));
    if (!segs_exp) return -5;
    auto segs = segs_exp.release();

    std::string json = "[";
    for (size_t i = 0; i < segs.size(); ++i) {
        if (i > 0) json += ",";
        json += "{\"startSec\":" + std::to_string(segs[i].start_sec)
              + ",\"endSec\":" + std::to_string(segs[i].end_sec)
              + ",\"text\":\"" + json_escape(segs[i].text.c_str()) + "\"}";
    }
    json += "]";

    size_t copy_len = json.size() < (out_capacity - 1) ? json.size() : (out_capacity - 1);
    std::memcpy(out_json, json.data(), copy_len);
    out_json[copy_len] = '\0';
    return static_cast<int>(segs.size());
}

} // extern "C"
"#;

#[cfg(unix)]
#[allow(unsafe_code)]
mod unix_ffi {
    use super::*;

    #[repr(C)]
    #[derive(Default)]
    struct HailoVersion {
        major: u32,
        minor: u32,
        revision: u32,
    }

    #[repr(C)]
    struct HailoVDeviceParams {
        device_count: u32,
        device_ids: *mut c_void,
        scheduling_algorithm: c_int,
        group_id: *const c_char,
        multi_process_service: bool,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct HailoChipTemperatureInfo {
        ts0_temperature: f32,
        ts1_temperature: f32,
        sample_count: u16,
    }

    #[repr(C)]
    struct HailoDeviceIdentity {
        protocol_version: u32,
        fw_major: u32,
        fw_minor: u32,
        fw_revision: u32,
        logger_version: u32,
        board_name_length: u8,
        board_name: [c_char; 32],
        is_release: bool,
        extended_context_switch_buffer: bool,
        extended_fw_check: bool,
        device_architecture: c_int,
        serial_number_length: u8,
        serial_number: [c_char; 16],
        part_number_length: u8,
        part_number: [c_char; 16],
        product_name_length: u8,
        product_name: [c_char; 42],
    }

    type FnGetLibVersion = unsafe extern "C" fn(*mut HailoVersion) -> c_int;
    type FnInitVDeviceParams = unsafe extern "C" fn(*mut HailoVDeviceParams) -> c_int;
    type FnCreateVDevice = unsafe extern "C" fn(*mut HailoVDeviceParams, *mut *mut c_void) -> c_int;
    type FnGetPhysicalDevices =
        unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut usize) -> c_int;
    type FnIdentify = unsafe extern "C" fn(*mut c_void, *mut HailoDeviceIdentity) -> c_int;
    type FnGetChipTemp = unsafe extern "C" fn(*mut c_void, *mut HailoChipTemperatureInfo) -> c_int;
    type FnReleaseVDevice = unsafe extern "C" fn(*mut c_void) -> c_int;

    // Hailo-8 C VStreams API symbols (used when InferModel C++ headers are absent)
    type FnCreateHefFile = unsafe extern "C" fn(*mut *mut c_void, *const c_char) -> c_int;
    type FnReleaseHef = unsafe extern "C" fn(*mut c_void) -> c_int;
    type FnInitConfigureParams = unsafe extern "C" fn(*mut c_void, c_int, *mut c_void) -> c_int;
    type FnConfigureVDevice = unsafe extern "C" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut *mut c_void,
        *mut usize,
    ) -> c_int;
    type FnSetSchedulerPriority = unsafe extern "C" fn(*mut c_void, u8, *const c_char) -> c_int;
    type FnMakeInputVStreamParams =
        unsafe extern "C" fn(*mut c_void, bool, c_int, *mut u8, *mut usize) -> c_int;
    type FnMakeOutputVStreamParams =
        unsafe extern "C" fn(*mut c_void, bool, c_int, *mut u8, *mut usize) -> c_int;
    type FnCreateInputVStreams =
        unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut c_void) -> c_int;
    type FnCreateOutputVStreams =
        unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut *mut c_void) -> c_int;
    type FnGetInputVStreamFrameSize = unsafe extern "C" fn(*mut c_void, *mut usize) -> c_int;
    type FnGetOutputVStreamFrameSize = unsafe extern "C" fn(*mut c_void, *mut usize) -> c_int;
    type FnVStreamWriteRaw = unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> c_int;
    type FnFlushInputVStream = unsafe extern "C" fn(*mut c_void) -> c_int;
    type FnVStreamReadRaw = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> c_int;
    type FnReleaseInputVStreams = unsafe extern "C" fn(*const *mut c_void, usize) -> c_int;
    type FnReleaseOutputVStreams = unsafe extern "C" fn(*const *mut c_void, usize) -> c_int;

    // InferModel C++ bridge symbols
    type FnBridgeCreate = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        u8,
        *mut c_uint,
        *mut c_uint,
        *mut c_uint,
        *mut usize,
        *mut usize,
        *mut c_uint,
        *mut c_int,
    ) -> *mut c_void;
    type FnBridgeInfer =
        unsafe extern "C" fn(*mut c_void, *const c_uchar, usize, *mut f32, usize, c_uint) -> c_int;
    type FnBridgeTemp = unsafe extern "C" fn(*mut c_void, *mut f32) -> c_int;
    type FnBridgeDestroy = unsafe extern "C" fn(*mut c_void);

    // GenAI C++ bridge symbols (Hailo-10H VLM, LLM, Speech2Text)
    type FnVlmFrameShape = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *mut c_uint,
        *mut c_uint,
        *mut c_uint,
    ) -> c_int;
    type FnVlmDescribe = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_uchar,
        usize,
        c_uint,
        *mut c_char,
        usize,
    ) -> c_int;
    type FnLlmGenerate = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        c_uint,
        *mut c_char,
        usize,
    ) -> c_int;
    type FnSpeech2Text = unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const f32,
        usize,
        *const c_char,
        c_int,
        *mut c_char,
        usize,
    ) -> c_int;

    fn open_libhailort() -> Option<*mut c_void> {
        let candidates = [
            "libhailort.so",
            "libhailort.so.5",
            "libhailort.so.4",
            "/usr/lib/libhailort.so",
            "/usr/lib/aarch64-linux-gnu/libhailort.so",
            "/usr/lib/x86_64-linux-gnu/libhailort.so",
        ];
        for path in candidates {
            if let Ok(cstr) = CString::new(path) {
                // SAFETY: `cstr` is a valid NUL-terminated string.
                let handle = unsafe { dl::dlopen(cstr.as_ptr(), dl::RTLD_NOW | dl::RTLD_LOCAL) };
                if !handle.is_null() {
                    return Some(handle);
                }
            }
        }
        None
    }

    unsafe fn load_sym<T>(handle: *mut c_void, name: &[u8]) -> Option<T> {
        let cname = CStr::from_bytes_with_nul(name).ok()?;
        // SAFETY: `handle` is an open dlopen handle and `cname` is NUL-terminated.
        let ptr = unsafe { dl::dlsym(handle, cname.as_ptr()) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: Caller ensures `T` is a function pointer matching the C ABI symbol.
        Some(unsafe { std::mem::transmute_copy(&ptr) })
    }

    /// Probe physical Hailo device identity and die temperature in-process via `libhailort.so`.
    pub fn probe_device_via_libhailort(group_id: &str) -> Option<LibHailoProbe> {
        let handle = open_libhailort()?;
        // SAFETY: Loading documented C ABI symbols from `libhailort.so`.
        unsafe {
            let get_ver: FnGetLibVersion = load_sym(handle, b"hailo_get_library_version\0")?;
            let init_vparams: FnInitVDeviceParams =
                load_sym(handle, b"hailo_init_vdevice_params\0")?;
            let create_vdev: FnCreateVDevice = load_sym(handle, b"hailo_create_vdevice\0")?;
            let get_phys: FnGetPhysicalDevices = load_sym(handle, b"hailo_get_physical_devices\0")?;
            let identify: FnIdentify = load_sym(handle, b"hailo_identify\0")?;
            let get_temp: Option<FnGetChipTemp> = load_sym(handle, b"hailo_get_chip_temperature\0");
            let release_vdev: FnReleaseVDevice = load_sym(handle, b"hailo_release_vdevice\0")?;

            let mut ver = HailoVersion::default();
            if get_ver(&mut ver) != 0 {
                return None;
            }
            let library_version = format!("{}.{}.{}", ver.major, ver.minor, ver.revision);

            let mut vparams: HailoVDeviceParams = std::mem::zeroed();
            if init_vparams(&mut vparams) != 0 {
                return None;
            }
            let group_cstr = CString::new(group_id).ok()?;
            vparams.group_id = group_cstr.as_ptr();
            vparams.scheduling_algorithm = 1; // HAILO_SCHEDULING_ALGORITHM_ROUND_ROBIN

            let mut vdev: *mut c_void = std::ptr::null_mut();
            if create_vdev(&mut vparams, &mut vdev) != 0 || vdev.is_null() {
                return None;
            }

            let mut phys: [*mut c_void; 4] = [std::ptr::null_mut(); 4];
            let mut phys_count: usize = 4;
            let mut result = None;

            if get_phys(vdev, phys.as_mut_ptr(), &mut phys_count) == 0 && phys_count > 0 {
                let mut id: HailoDeviceIdentity = std::mem::zeroed();
                if identify(phys[0], &mut id) == 0 {
                    let firmware_version =
                        format!("{}.{}.{}", id.fw_major, id.fw_minor, id.fw_revision);
                    let architecture = match id.device_architecture {
                        0 | 1 => "HAILO8".to_owned(),
                        2 => "HAILO8L".to_owned(),
                        3 => "HAILO15H".to_owned(),
                        4 => "HAILO15L".to_owned(),
                        5 => "HAILO15M".to_owned(),
                        6 => "HAILO10H".to_owned(),
                        7 => "HAILO12L".to_owned(),
                        other => format!("HAILO_ARCH_{other}"),
                    };
                    let mut chip_temperature_c = None;
                    if let Some(temp_fn) = get_temp {
                        let mut tinfo = HailoChipTemperatureInfo {
                            ts0_temperature: 0.0,
                            ts1_temperature: 0.0,
                            sample_count: 0,
                        };
                        if temp_fn(phys[0], &mut tinfo) == 0 && tinfo.ts0_temperature > -40.0 {
                            chip_temperature_c = Some(tinfo.ts0_temperature);
                        }
                    }
                    result = Some(LibHailoProbe {
                        library_version,
                        firmware_version,
                        architecture,
                        chip_temperature_c,
                    });
                }
            }

            let _ = release_vdev(vdev);
            result
        }
    }

    enum ActiveBackend {
        InferModelBridge {
            session: *mut c_void,
            fn_infer: FnBridgeInfer,
            fn_temp: FnBridgeTemp,
            fn_destroy: FnBridgeDestroy,
        },
        Hailo8CAbiVStreams {
            vdev: *mut c_void,
            hef: *mut c_void,
            in_vstream: *mut c_void,
            out_vstream: *mut c_void,
            fn_write: FnVStreamWriteRaw,
            fn_flush: FnFlushInputVStream,
            fn_read: FnVStreamReadRaw,
            fn_rel_in: FnReleaseInputVStreams,
            fn_rel_out: FnReleaseOutputVStreams,
            fn_rel_hef: FnReleaseHef,
            fn_rel_vdev: FnReleaseVDevice,
        },
    }

    pub struct UnixHailoSession {
        backend: ActiveBackend,
        pub arch_label: String,
        pub model_name: String,
        pub width: u32,
        pub height: u32,
        pub channels: u32,
        pub in_bytes: usize,
        pub out_bytes: usize,
        pub num_classes: u32,
        pub is_nms: bool,
        out_buffer: Vec<f32>,
    }

    // SAFETY: `UnixHailoSession` is always accessed behind a `Mutex` in `HailoEngine`.
    unsafe impl Send for UnixHailoSession {}

    impl Drop for UnixHailoSession {
        fn drop(&mut self) {
            // SAFETY: Releases owned HailoRT session handles created in `open`.
            unsafe {
                match self.backend {
                    ActiveBackend::InferModelBridge { session, fn_destroy, .. } => {
                        if !session.is_null() {
                            fn_destroy(session);
                        }
                    }
                    ActiveBackend::Hailo8CAbiVStreams {
                        vdev,
                        hef,
                        in_vstream,
                        out_vstream,
                        fn_rel_in,
                        fn_rel_out,
                        fn_rel_hef,
                        fn_rel_vdev,
                        ..
                    } => {
                        let ins = [in_vstream];
                        let outs = [out_vstream];
                        let _ = fn_rel_in(ins.as_ptr(), 1);
                        let _ = fn_rel_out(outs.as_ptr(), 1);
                        let _ = fn_rel_hef(hef);
                        let _ = fn_rel_vdev(vdev);
                    }
                }
            }
        }
    }

    static BRIDGE_HANDLE: OnceLock<Option<usize>> = OnceLock::new();

    fn ensure_infer_bridge_loaded() -> Option<*mut c_void> {
        let addr_opt = *BRIDGE_HANDLE.get_or_init(|| {
            let so_path = Path::new("/tmp/mbl_hailo_infer_bridge_v3.so");
            if !so_path.exists() {
                if !Path::new("/usr/include/hailo/hailort.hpp").exists() {
                    return None;
                }
                let cpp_path = Path::new("/tmp/mbl_hailo_infer_bridge_v3.cpp");
                std::fs::write(cpp_path, INFER_BRIDGE_CPP_SOURCE).ok()?;
                let status = Command::new("g++")
                    .args([
                        "-shared",
                        "-fPIC",
                        "-O2",
                        "-std=c++17",
                        "/tmp/mbl_hailo_infer_bridge_v3.cpp",
                        "-lhailort",
                        "-o",
                        "/tmp/mbl_hailo_infer_bridge_v3.so.tmp",
                    ])
                    .status()
                    .ok()?;
                let _ = std::fs::remove_file(cpp_path);
                if !status.success() {
                    return None;
                }
                std::fs::rename("/tmp/mbl_hailo_infer_bridge_v3.so.tmp", so_path).ok()?;
            }
            let cstr = CString::new(so_path.to_str()?).ok()?;
            // SAFETY: `cstr` is a valid NUL-terminated path to the compiled bridge shared library.
            let handle = unsafe { dl::dlopen(cstr.as_ptr(), dl::RTLD_NOW | dl::RTLD_LOCAL) };
            if handle.is_null() { None } else { Some(handle as usize) }
        });
        addr_opt.map(|a| a as *mut c_void)
    }

    /// Describe an image using a Hailo-10H Vision-Language Model (`hailort::genai::VLM`: Qwen2-VL / Qwen3-VL).
    pub fn run_vlm_describe(
        hef_path: &Path,
        group_id: &str,
        img: &DynamicImage,
        prompt: &str,
        max_tokens: u32,
    ) -> Option<(String, String)> {
        let bridge = ensure_infer_bridge_loaded()?;
        let model_name =
            hef_path.file_name().and_then(|f| f.to_str()).unwrap_or("vlm.hef").to_owned();
        // SAFETY: Calling C ABI functions from the compiled HailoRT GenAI bridge.
        unsafe {
            let fn_shape: FnVlmFrameShape = load_sym(bridge, b"mbl_hailo_genai_vlm_frame_shape\0")?;
            let fn_desc: FnVlmDescribe = load_sym(bridge, b"mbl_hailo_genai_vlm_describe\0")?;

            let hef_cstr = CString::new(hef_path.to_string_lossy().as_bytes()).ok()?;
            let grp_cstr = CString::new(group_id).ok()?;
            let prompt_cstr = CString::new(prompt).ok()?;

            let mut w: c_uint = 336;
            let mut h: c_uint = 336;
            let mut c: c_uint = 3;
            let _ = fn_shape(hef_cstr.as_ptr(), grp_cstr.as_ptr(), &mut w, &mut h, &mut c);

            let resized = img.resize_exact(w.max(1), h.max(1), FilterType::Triangle).to_rgb8();
            let raw_rgb = resized.as_raw();
            let mut out_buf = vec![0u8; 8192];
            let rc = fn_desc(
                hef_cstr.as_ptr(),
                grp_cstr.as_ptr(),
                prompt_cstr.as_ptr(),
                raw_rgb.as_ptr(),
                raw_rgb.len(),
                max_tokens.max(16),
                out_buf.as_mut_ptr().cast::<c_char>(),
                out_buf.len(),
            );
            if rc <= 0 {
                return None;
            }
            let text = String::from_utf8_lossy(&out_buf[..(rc as usize)]).trim().to_owned();
            if text.is_empty() { None } else { Some((text, format!("hailort-vlm({model_name})"))) }
        }
    }

    /// Generate text using a Hailo-10H Large Language Model (`hailort::genai::LLM`: Qwen2.5, Qwen3, Llama3.2, Gemma4, DeepSeek-R1).
    pub fn run_llm_generate(
        hef_path: &Path,
        group_id: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Option<(String, String)> {
        let bridge = ensure_infer_bridge_loaded()?;
        let model_name =
            hef_path.file_name().and_then(|f| f.to_str()).unwrap_or("llm.hef").to_owned();
        // SAFETY: Calling C ABI function `mbl_hailo_genai_llm_generate` from the HailoRT GenAI bridge.
        unsafe {
            let fn_llm: FnLlmGenerate = load_sym(bridge, b"mbl_hailo_genai_llm_generate\0")?;
            let hef_cstr = CString::new(hef_path.to_string_lossy().as_bytes()).ok()?;
            let grp_cstr = CString::new(group_id).ok()?;
            let prompt_cstr = CString::new(prompt).ok()?;
            let mut out_buf = vec![0u8; 16384];

            let rc = fn_llm(
                hef_cstr.as_ptr(),
                grp_cstr.as_ptr(),
                prompt_cstr.as_ptr(),
                max_tokens.max(32),
                out_buf.as_mut_ptr().cast::<c_char>(),
                out_buf.len(),
            );
            if rc <= 0 {
                return None;
            }
            let text = String::from_utf8_lossy(&out_buf[..(rc as usize)]).trim().to_owned();
            if text.is_empty() { None } else { Some((text, format!("hailort-llm({model_name})"))) }
        }
    }

    /// Transcribe or translate 16 kHz mono float32 PCM audio using Hailo-10H `hailort::genai::Speech2Text` (`Whisper`).
    pub fn run_speech2text(
        hef_path: &Path,
        group_id: &str,
        pcm_f32_16k: &[f32],
        language: Option<&str>,
        translate_to_english: bool,
    ) -> Option<(Vec<HailoSpeechSegment>, String)> {
        if pcm_f32_16k.is_empty() {
            return None;
        }
        let bridge = ensure_infer_bridge_loaded()?;
        let model_name =
            hef_path.file_name().and_then(|f| f.to_str()).unwrap_or("whisper.hef").to_owned();
        // SAFETY: Calling C ABI function `mbl_hailo_genai_speech2text` from the HailoRT GenAI bridge.
        unsafe {
            let fn_s2t: FnSpeech2Text = load_sym(bridge, b"mbl_hailo_genai_speech2text\0")?;
            let hef_cstr = CString::new(hef_path.to_string_lossy().as_bytes()).ok()?;
            let grp_cstr = CString::new(group_id).ok()?;
            let lang_cstr = CString::new(language.unwrap_or("")).ok()?;
            let mut out_buf = vec![0u8; 65536];

            let rc = fn_s2t(
                hef_cstr.as_ptr(),
                grp_cstr.as_ptr(),
                pcm_f32_16k.as_ptr(),
                pcm_f32_16k.len(),
                lang_cstr.as_ptr(),
                i32::from(translate_to_english),
                out_buf.as_mut_ptr().cast::<c_char>(),
                out_buf.len(),
            );
            if rc < 0 {
                return None;
            }
            let nul_pos = out_buf.iter().position(|&b| b == 0).unwrap_or(out_buf.len());
            let json_str = std::str::from_utf8(&out_buf[..nul_pos]).ok()?;
            let parsed: Vec<serde_json::Value> = serde_json::from_str(json_str).ok()?;
            let mut segs = Vec::with_capacity(parsed.len());
            for item in parsed {
                let start_sec = item.get("startSec").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let end_sec = item.get("endSec").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let text =
                    item.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_owned();
                if !text.is_empty() {
                    segs.push(HailoSpeechSegment { start_sec, end_sec, text });
                }
            }
            Some((segs, format!("hailort-whisper({model_name})")))
        }
    }

    impl UnixHailoSession {
        pub fn open(
            hef_path: &Path,
            group_id: &str,
            priority: u8,
            arch_label: &str,
        ) -> Option<Self> {
            let model_name =
                hef_path.file_name().and_then(|f| f.to_str()).unwrap_or("model.hef").to_owned();

            // 1. Try Tier 1: Unified HailoRT InferModel bridge (works on both Hailo-10H and Hailo-8/8L)
            if let Some(bridge) = ensure_infer_bridge_loaded() {
                // SAFETY: Calling C ABI symbols from `mbl_hailo_infer_bridge_v1.so`.
                unsafe {
                    let fn_create: FnBridgeCreate =
                        load_sym(bridge, b"mbl_hailo_session_create\0")?;
                    let fn_infer: FnBridgeInfer = load_sym(bridge, b"mbl_hailo_session_infer\0")?;
                    let fn_temp: FnBridgeTemp =
                        load_sym(bridge, b"mbl_hailo_session_temperature\0")?;
                    let fn_destroy: FnBridgeDestroy =
                        load_sym(bridge, b"mbl_hailo_session_destroy\0")?;

                    let hef_cstr = CString::new(hef_path.to_string_lossy().as_bytes()).ok()?;
                    let grp_cstr = CString::new(group_id).ok()?;
                    let mut w: c_uint = 0;
                    let mut h: c_uint = 0;
                    let mut c: c_uint = 0;
                    let mut in_bytes: usize = 0;
                    let mut out_bytes: usize = 0;
                    let mut num_classes: c_uint = 0;
                    let mut is_nms: c_int = 0;

                    let sess = fn_create(
                        hef_cstr.as_ptr(),
                        grp_cstr.as_ptr(),
                        priority,
                        &mut w,
                        &mut h,
                        &mut c,
                        &mut in_bytes,
                        &mut out_bytes,
                        &mut num_classes,
                        &mut is_nms,
                    );
                    if !sess.is_null() && in_bytes > 0 && out_bytes >= 4 {
                        let float_len = out_bytes / std::mem::size_of::<f32>();
                        return Some(Self {
                            backend: ActiveBackend::InferModelBridge {
                                session: sess,
                                fn_infer,
                                fn_temp,
                                fn_destroy,
                            },
                            arch_label: arch_label.to_owned(),
                            model_name,
                            width: w.max(1),
                            height: h.max(1),
                            channels: c.max(3),
                            in_bytes,
                            out_bytes,
                            num_classes: if num_classes > 0 { num_classes } else { 80 },
                            is_nms: is_nms != 0,
                            out_buffer: vec![0.0f32; float_len],
                        });
                    }
                }
            }

            // 2. Try Tier 2: Pure C `libhailort.so` VStreams API (Hailo-8 / Hailo-8L runtime-only hosts)
            Self::open_c_vstreams(hef_path, group_id, priority, arch_label, model_name)
        }

        fn open_c_vstreams(
            hef_path: &Path,
            group_id: &str,
            priority: u8,
            arch_label: &str,
            model_name: String,
        ) -> Option<Self> {
            let handle = open_libhailort()?;
            // SAFETY: Calling HailoRT C API symbols for VDevice + VStream setup on Hailo-8/8L.
            unsafe {
                let init_vparams: FnInitVDeviceParams =
                    load_sym(handle, b"hailo_init_vdevice_params\0")?;
                let create_vdev: FnCreateVDevice = load_sym(handle, b"hailo_create_vdevice\0")?;
                let release_vdev: FnReleaseVDevice = load_sym(handle, b"hailo_release_vdevice\0")?;
                let create_hef: FnCreateHefFile = load_sym(handle, b"hailo_create_hef_file\0")?;
                let release_hef: FnReleaseHef = load_sym(handle, b"hailo_release_hef\0")?;
                let init_cparams: FnInitConfigureParams =
                    load_sym(handle, b"hailo_init_configure_params\0")?;
                let config_vdev: FnConfigureVDevice =
                    load_sym(handle, b"hailo_configure_vdevice\0")?;
                let set_prio: FnSetSchedulerPriority =
                    load_sym(handle, b"hailo_set_scheduler_priority\0")?;
                let make_in: FnMakeInputVStreamParams =
                    load_sym(handle, b"hailo_make_input_vstream_params\0")?;
                let make_out: FnMakeOutputVStreamParams =
                    load_sym(handle, b"hailo_make_output_vstream_params\0")?;
                let create_in: FnCreateInputVStreams =
                    load_sym(handle, b"hailo_create_input_vstreams\0")?;
                let create_out: FnCreateOutputVStreams =
                    load_sym(handle, b"hailo_create_output_vstreams\0")?;
                let get_in_sz: FnGetInputVStreamFrameSize =
                    load_sym(handle, b"hailo_get_input_vstream_frame_size\0")?;
                let get_out_sz: FnGetOutputVStreamFrameSize =
                    load_sym(handle, b"hailo_get_output_vstream_frame_size\0")?;
                let fn_write: FnVStreamWriteRaw =
                    load_sym(handle, b"hailo_vstream_write_raw_buffer\0")?;
                let fn_flush: FnFlushInputVStream =
                    load_sym(handle, b"hailo_flush_input_vstream\0")?;
                let fn_read: FnVStreamReadRaw =
                    load_sym(handle, b"hailo_vstream_read_raw_buffer\0")?;
                let fn_rel_in: FnReleaseInputVStreams =
                    load_sym(handle, b"hailo_release_input_vstreams\0")?;
                let fn_rel_out: FnReleaseOutputVStreams =
                    load_sym(handle, b"hailo_release_output_vstreams\0")?;

                let mut vparams: HailoVDeviceParams = std::mem::zeroed();
                if init_vparams(&mut vparams) != 0 {
                    return None;
                }
                let grp_cstr = CString::new(group_id).ok()?;
                vparams.group_id = grp_cstr.as_ptr();
                vparams.scheduling_algorithm = 1; // ROUND_ROBIN

                let mut vdev: *mut c_void = std::ptr::null_mut();
                if create_vdev(&mut vparams, &mut vdev) != 0 || vdev.is_null() {
                    return None;
                }

                let hef_cstr = CString::new(hef_path.to_string_lossy().as_bytes()).ok()?;
                let mut hef: *mut c_void = std::ptr::null_mut();
                if create_hef(&mut hef, hef_cstr.as_ptr()) != 0 || hef.is_null() {
                    let _ = release_vdev(vdev);
                    return None;
                }

                // Opaque aligned buffer large enough for hailo_configure_params_t across 4.x and 5.x
                let mut cparams = vec![0u64; 16384];
                if init_cparams(
                    hef,
                    0, /* HAILO_STREAM_INTERFACE_PCIE */
                    cparams.as_mut_ptr().cast(),
                ) != 0
                {
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let mut ng: *mut c_void = std::ptr::null_mut();
                let mut ng_count: usize = 1;
                if config_vdev(vdev, hef, cparams.as_mut_ptr().cast(), &mut ng, &mut ng_count) != 0
                    || ng.is_null()
                {
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }
                let _ = set_prio(ng, priority, std::ptr::null());

                // Each hailo_*_vstream_params_by_name_t is 156 bytes; allocate 8 entries (2048 bytes)
                let mut in_params_buf = vec![0u8; 2048];
                let mut in_count: usize = 8;
                if make_in(ng, true, 1 /* UINT8 */, in_params_buf.as_mut_ptr(), &mut in_count) != 0
                    || in_count == 0
                {
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let mut out_params_buf = vec![0u8; 2048];
                let mut out_count: usize = 8;
                if make_out(
                    ng,
                    true,
                    3, /* FLOAT32 */
                    out_params_buf.as_mut_ptr(),
                    &mut out_count,
                ) != 0
                    || out_count == 0
                {
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let mut in_vstreams = [std::ptr::null_mut(); 8];
                if create_in(ng, in_params_buf.as_ptr(), in_count, in_vstreams.as_mut_ptr()) != 0 {
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let mut out_vstreams = [std::ptr::null_mut(); 8];
                if create_out(ng, out_params_buf.as_ptr(), out_count, out_vstreams.as_mut_ptr())
                    != 0
                {
                    let _ = fn_rel_in(in_vstreams.as_ptr(), in_count);
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let mut in_bytes: usize = 0;
                let mut out_bytes: usize = 0;
                let _ = get_in_sz(in_vstreams[0], &mut in_bytes);
                let _ = get_out_sz(out_vstreams[0], &mut out_bytes);
                if in_bytes == 0 || out_bytes < 4 {
                    let _ = fn_rel_in(in_vstreams.as_ptr(), in_count);
                    let _ = fn_rel_out(out_vstreams.as_ptr(), out_count);
                    let _ = release_hef(hef);
                    let _ = release_vdev(vdev);
                    return None;
                }

                let side = ((in_bytes / 3) as f64).sqrt().round() as u32;
                let float_len = out_bytes / std::mem::size_of::<f32>();
                let is_nms = out_bytes == 80 * (4 + 100 * 20) || model_name.contains("yolo");

                Some(Self {
                    backend: ActiveBackend::Hailo8CAbiVStreams {
                        vdev,
                        hef,
                        in_vstream: in_vstreams[0],
                        out_vstream: out_vstreams[0],
                        fn_write,
                        fn_flush,
                        fn_read,
                        fn_rel_in,
                        fn_rel_out,
                        fn_rel_hef: release_hef,
                        fn_rel_vdev: release_vdev,
                    },
                    arch_label: arch_label.to_owned(),
                    model_name,
                    width: side.max(1),
                    height: side.max(1),
                    channels: 3,
                    in_bytes,
                    out_bytes,
                    num_classes: 80,
                    is_nms,
                    out_buffer: vec![0.0f32; float_len],
                })
            }
        }

        pub fn infer(&mut self, img: &DynamicImage) -> Result<NpuFrameResult, String> {
            let resized = img.resize_exact(self.width, self.height, FilterType::Triangle).to_rgb8();
            let raw_rgb = resized.as_raw();
            self.out_buffer.fill(0.0);

            // SAFETY: Executing hardware inference with validated buffer sizes.
            unsafe {
                match self.backend {
                    ActiveBackend::InferModelBridge { session, fn_infer, fn_temp, .. } => {
                        let rc = fn_infer(
                            session,
                            raw_rgb.as_ptr(),
                            raw_rgb.len(),
                            self.out_buffer.as_mut_ptr(),
                            self.out_bytes,
                            2000,
                        );
                        if rc != 0 {
                            return Err(format!("Hailo InferModel run failed with status {rc}"));
                        }
                        let mut temp_c: f32 = 0.0;
                        let chip_temp_c =
                            if fn_temp(session, &mut temp_c) == 0 { Some(temp_c) } else { None };
                        Ok(self.build_result(chip_temp_c))
                    }
                    ActiveBackend::Hailo8CAbiVStreams {
                        in_vstream,
                        out_vstream,
                        fn_write,
                        fn_flush,
                        fn_read,
                        ..
                    } => {
                        let out_ptr = self.out_buffer.as_mut_ptr().cast::<c_void>() as usize;
                        let out_vs_addr = out_vstream as usize;
                        let out_bytes = self.out_bytes;
                        let reader = std::thread::spawn(move || {
                            fn_read(out_vs_addr as *mut c_void, out_ptr as *mut c_void, out_bytes)
                        });
                        let wrc = fn_write(
                            in_vstream,
                            raw_rgb.as_ptr().cast::<c_void>(),
                            raw_rgb.len().min(self.in_bytes),
                        );
                        let _ = fn_flush(in_vstream);
                        let rrc = reader.join().unwrap_or(-1);
                        if wrc != 0 || rrc != 0 {
                            return Err(format!(
                                "Hailo VStream inference failed (write={wrc}, read={rrc})"
                            ));
                        }
                        Ok(self.build_result(None))
                    }
                }
            }
        }

        fn build_result(&self, chip_temp_c: Option<f32>) -> NpuFrameResult {
            let backend_name = format!("hailort-npu({},{})", self.arch_label, self.model_name);
            if self.is_nms {
                let detections =
                    parse_nms_by_class(&self.out_buffer, self.num_classes as usize, 0.22);
                NpuFrameResult {
                    backend_name,
                    model_name: self.model_name.clone(),
                    detections,
                    dense_features: Vec::new(),
                    chip_temp_c,
                }
            } else {
                NpuFrameResult {
                    backend_name,
                    model_name: self.model_name.clone(),
                    detections: Vec::new(),
                    dense_features: self.out_buffer.clone(),
                    chip_temp_c,
                }
            }
        }
    }
}

/// Parse a `HAILO_FORMAT_ORDER_HAILO_NMS_BY_CLASS` (`FLOAT32`) output buffer into detections.
///
/// Layout per class `c` in `0..num_classes`:
/// `float32_t bbox_count` followed immediately by `bbox_count` instances of
/// `hailo_bbox_float32_t { y_min, x_min, y_max, x_max, score }`.
pub fn parse_nms_by_class(
    floats: &[f32],
    num_classes: usize,
    score_threshold: f32,
) -> Vec<CocoDetection> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    let mut class_id = 0usize;

    while class_id < num_classes && idx < floats.len() {
        let count_f = floats[idx];
        idx += 1;
        if !count_f.is_finite() || count_f < 0.0 || count_f > 500.0 {
            break;
        }
        let count = (count_f + 0.5) as usize;
        for _ in 0..count {
            if idx + 5 > floats.len() {
                break;
            }
            let y_min = floats[idx].clamp(0.0, 1.0);
            let x_min = floats[idx + 1].clamp(0.0, 1.0);
            let y_max = floats[idx + 2].clamp(0.0, 1.0);
            let x_max = floats[idx + 3].clamp(0.0, 1.0);
            let score = floats[idx + 4];
            idx += 5;

            if score >= score_threshold && score <= 1.0 {
                let label = COCO_CLASSES.get(class_id).copied().unwrap_or("object");
                out.push(CocoDetection {
                    class_id,
                    label,
                    score,
                    bbox: [y_min, x_min, y_max, x_max],
                });
            }
        }
        class_id += 1;
    }

    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// Map COCO object detections from the Hailo NPU into MBL semantic tags and face/person count.
pub fn map_detections_to_tags_and_faces(detections: &[CocoDetection]) -> (Vec<String>, u32) {
    let mut tags = Vec::new();
    let mut person_count = 0u32;

    let mut add_tag = |t: &str| {
        if !tags.iter().any(|existing| existing == t) {
            tags.push(t.to_owned());
        }
    };

    for det in detections {
        match det.label {
            "person" => {
                person_count = person_count.saturating_add(1);
                add_tag("face");
                add_tag("person");
            }
            "car" | "truck" | "bus" | "motorcycle" | "bicycle" => {
                add_tag("car_chase");
                add_tag("city");
                add_tag(det.label);
            }
            "boat" | "surfboard" => {
                add_tag("ocean");
                add_tag(det.label);
            }
            "airplane" | "bird" | "kite" => {
                add_tag("aerial");
                add_tag(det.label);
            }
            "skis" | "snowboard" => {
                add_tag("snow");
                add_tag("mountain");
                add_tag(det.label);
            }
            "umbrella" => {
                add_tag("rain");
                add_tag(det.label);
            }
            "traffic light" | "fire hydrant" | "stop sign" | "parking meter" => {
                add_tag("city");
                add_tag(det.label);
            }
            "chair" | "couch" | "bed" | "dining table" | "tv" | "laptop" | "refrigerator"
            | "book" | "microwave" | "oven" => {
                add_tag("indoor");
                add_tag(det.label);
            }
            "cat" | "dog" | "horse" | "sheep" | "cow" | "elephant" | "bear" | "zebra"
            | "giraffe" | "potted plant" => {
                add_tag("forest");
                add_tag(det.label);
            }
            other => {
                add_tag(other);
            }
        }
    }

    (tags, person_count.min(16))
}

/// Thread-safe handle managing the persistent cooperative Hailo NPU session.
#[derive(Clone, Default)]
pub struct HailoNativeRuntime {
    group_id: String,
    priority: u8,
    #[cfg(unix)]
    session: Arc<Mutex<Option<(std::path::PathBuf, unix_ffi::UnixHailoSession)>>>,
}

impl std::fmt::Debug for HailoNativeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HailoNativeRuntime").field("group_id", &self.group_id).finish()
    }
}

impl HailoNativeRuntime {
    pub fn new(config: &HailoConfig) -> Self {
        let group_id = if config.vdevice_group_id.trim().is_empty() {
            "SHARED".to_owned()
        } else {
            config.vdevice_group_id.clone()
        };
        let priority = if config.mode == HailoSharingMode::Exclusive { 16 } else { 8 };
        #[cfg(unix)]
        {
            if matches!(config.mode, HailoSharingMode::Disabled | HailoSharingMode::OffloadOnly) {
                return Self { group_id, priority, session: Arc::new(Mutex::new(None)) };
            }
            let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
            if !fp.npu_profile.is_present() {
                return Self { group_id, priority, session: Arc::new(Mutex::new(None)) };
            }
            let Some(hef_path) = discover_hef_model_for_profile(Path::new("/"), fp.npu_profile)
            else {
                return Self { group_id, priority, session: Arc::new(Mutex::new(None)) };
            };
            let arch_label = fp
                .hailo_devices
                .first()
                .map(|d| d.architecture.as_str())
                .unwrap_or_else(|| fp.npu_profile.as_str());
            let sess = unix_ffi::UnixHailoSession::open(&hef_path, &group_id, priority, arch_label);
            if let Some(ref s) = sess {
                tracing::info!(
                    arch = %s.arch_label,
                    model = %s.model_name,
                    group_id = %group_id,
                    shape = %format!("{}x{}x{}", s.width, s.height, s.channels),
                    "Initialized native Hailo NPU inference session"
                );
            }
            Self { group_id, priority, session: Arc::new(Mutex::new(sess.map(|s| (hef_path, s)))) }
        }
        #[cfg(not(unix))]
        {
            Self { group_id, priority }
        }
    }

    #[cfg(unix)]
    fn ensure_vision_session(
        &self,
        guard: &mut Option<(std::path::PathBuf, unix_ffi::UnixHailoSession)>,
    ) {
        let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
        if !fp.npu_profile.is_present() {
            return;
        }
        let Some(target_hef) = discover_hef_model_for_profile(Path::new("/"), fp.npu_profile)
        else {
            return;
        };
        if guard.as_ref().is_some_and(|(current_path, _)| current_path == &target_hef) {
            return;
        }
        // Drop previous session before opening the newly selected `.hef` model
        *guard = None;
        let arch_label = fp
            .hailo_devices
            .first()
            .map(|d| d.architecture.as_str())
            .unwrap_or_else(|| fp.npu_profile.as_str());
        let group_id = if self.group_id.is_empty() { "SHARED" } else { &self.group_id };
        if let Some(sess) =
            unix_ffi::UnixHailoSession::open(&target_hef, group_id, self.priority, arch_label)
        {
            tracing::info!(
                arch = %sess.arch_label,
                model = %sess.model_name,
                path = %target_hef.display(),
                "Switched active Hailo NPU vision HEF model"
            );
            *guard = Some((target_hef, sess));
        }
    }

    /// Return the active backend identifier if a Hailo NPU session is loaded.
    pub fn active_backend_name(&self) -> Option<String> {
        #[cfg(unix)]
        {
            let mut guard = self.session.lock().ok()?;
            self.ensure_vision_session(&mut guard);
            let (_, sess) = guard.as_ref()?;
            Some(format!("hailort-npu({},{})", sess.arch_label, sess.model_name))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Run a single image frame through the Hailo-8 / 8L / 10H NPU if an active session is open.
    pub fn infer_image(&self, img: &DynamicImage) -> Option<NpuFrameResult> {
        #[cfg(unix)]
        {
            let mut guard = self.session.lock().ok()?;
            self.ensure_vision_session(&mut guard);
            let (_, sess) = guard.as_mut()?;
            match sess.infer(img) {
                Ok(res) => Some(res),
                Err(e) => {
                    tracing::warn!(error = %e, "Hailo NPU frame inference failed; falling back to CPU heuristic");
                    None
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = img;
            None
        }
    }

    /// Describe an image using a Hailo-10H Vision-Language Model (`hailort::genai::VLM`:
    /// `Qwen3-VL-2B-Instruct.hef`, `Qwen2-VL-2B-Instruct.hef`, or `Qwen3-VL-4B-Instruct.hef`)
    /// if a VLM `.hef` is installed on the host.
    pub fn describe_image_vlm(
        &self,
        img: &DynamicImage,
        prompt: &str,
        max_tokens: u32,
    ) -> Option<(String, String)> {
        #[cfg(unix)]
        {
            let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
            let hef_path =
                mbl_hwaccel::hailo::discover_vlm_hef_model(Path::new("/"), fp.npu_profile)?;
            let group_id = if self.group_id.is_empty() { "SHARED" } else { &self.group_id };
            unix_ffi::run_vlm_describe(&hef_path, group_id, img, prompt, max_tokens)
        }
        #[cfg(not(unix))]
        {
            let _ = (img, prompt, max_tokens);
            None
        }
    }

    /// Generate text directly on Hailo-10H using `hailort::genai::LLM` (`Qwen2.5-1.5B-Instruct.hef`,
    /// `Qwen3-1.7B-Instruct.hef`, `Gemma4-E2B-Instruct.hef`, `Llama3.2-1B-Instruct.hef`,
    /// `DeepSeek-R1-Distill-Qwen-1.5B.hef`, `Qwen3-4B-Instruct-2507.hef`, etc.) without requiring
    /// the external `hailo-ollama` daemon.
    pub fn generate_text_llm(&self, prompt: &str, max_tokens: u32) -> Option<(String, String)> {
        #[cfg(unix)]
        {
            let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
            let hef_path =
                mbl_hwaccel::hailo::discover_llm_hef_model(Path::new("/"), fp.npu_profile)?;
            let group_id = if self.group_id.is_empty() { "SHARED" } else { &self.group_id };
            unix_ffi::run_llm_generate(&hef_path, group_id, prompt, max_tokens)
        }
        #[cfg(not(unix))]
        {
            let _ = (prompt, max_tokens);
            None
        }
    }

    /// Transcribe or translate 16 kHz mono float32 PCM audio on Hailo-10H using
    /// `hailort::genai::Speech2Text` (`Whisper-Base.hef`, `Whisper-Small.hef`, `Whisper-Tiny.hef`,
    /// or `Whisper-Medium.hef`).
    pub fn transcribe_audio_whisper(
        &self,
        pcm_f32_16k: &[f32],
        language: Option<&str>,
        translate_to_english: bool,
    ) -> Option<(Vec<HailoSpeechSegment>, String)> {
        #[cfg(unix)]
        {
            let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
            let hef_path =
                mbl_hwaccel::hailo::discover_whisper_hef_model(Path::new("/"), fp.npu_profile)?;
            let group_id = if self.group_id.is_empty() { "SHARED" } else { &self.group_id };
            unix_ffi::run_speech2text(
                &hef_path,
                group_id,
                pcm_f32_16k,
                language,
                translate_to_english,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (pcm_f32_16k, language, translate_to_english);
            None
        }
    }

    /// Probe physical Hailo device identity and temperature via `libhailort.so`.
    pub fn probe_device(group_id: &str) -> Option<LibHailoProbe> {
        #[cfg(unix)]
        {
            unix_ffi::probe_device_via_libhailort(group_id)
        }
        #[cfg(not(unix))]
        {
            let _ = group_id;
            None
        }
    }
}
