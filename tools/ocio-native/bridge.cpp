// Independently authored C ABI. No C++ exception or allocator ownership crosses it.
#include <OpenColorIO/OpenColorIO.h>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace ocio = OCIO_NAMESPACE;
#if defined(_WIN32)
#define MOTION_EXPORT extern "C" __declspec(dllexport)
#else
#define MOTION_EXPORT extern "C" __attribute__((visibility("default")))
#endif

struct Error { uint32_t kind; char message[2048]; };
struct Table {
    const char *texture, *sampler;
    uint32_t binding, width, height, channels, interpolation;
    const float *values;
    size_t count;
};
struct Program {
    ocio::ConstCPUProcessorRcPtr cpu;
    ocio::GpuShaderDescRcPtr gpu;
};
struct Admission : std::runtime_error { using std::runtime_error::runtime_error; };
using LogFunction = void (*)(const char *, size_t);
static std::atomic<LogFunction> logger{nullptr};

static size_t length(const char *text, size_t limit) {
    if (!text) throw Admission("OCIO returned a null string");
    size_t count = 0;
    while (count <= limit && text[count]) ++count;
    if (count > limit) throw Admission("OCIO string exceeds admission capacity");
    return count;
}
static void failure(Error *error, uint32_t kind, const char *message) noexcept {
    error->kind = kind;
    size_t count = 0;
    if (message) while (count < sizeof(error->message)-1 && message[count]) ++count;
    if (message) std::memcpy(error->message, message, count);
    error->message[count] = 0;
}
template<class F> static bool checked(Error *error, F action) noexcept {
    *error = {};
    try { action(); return true; }
    catch (const Admission &e) { failure(error, 1, e.what()); }
    catch (const ocio::Exception &e) { failure(error, 2, e.what()); }
    catch (const std::bad_alloc &e) { failure(error, 3, e.what()); }
    catch (const std::exception &e) { failure(error, 4, e.what()); }
    catch (...) { failure(error, 5, "unknown native OCIO exception"); }
    return false;
}
static void log_message(const char *message) noexcept {
    if (auto callback = logger.load(std::memory_order_acquire)) {
        size_t count = 0;
        if (message) while (count < 2048 && message[count]) ++count;
        callback(message, count);
    }
}
MOTION_EXPORT bool motion_ocio_initialize(LogFunction callback, Error *error) noexcept {
    return checked(error, [&] {
        const auto version = static_cast<uint32_t>(ocio::GetVersionHex());
        if (version != 0x02060000) throw Admission("loaded OCIO is not 2.6.0");
        if (!callback) throw Admission("native log callback must not be null");
        // The pinned setter copies the callback under its own mutex. Publish
        // the Rust callback only after registration succeeds; no throwing work
        // follows that publication. A failure can therefore unload safely.
        ocio::SetLoggingFunction(log_message);
        logger.store(callback, std::memory_order_release);
    });
}
static void scan(const ocio::ConstTransformRcPtr &transform, unsigned depth, unsigned &nodes) {
    if (!transform) return;
    if (depth > 32 || ++nodes > 8192) throw Admission("configuration transform traversal exceeds capacity");
    if (std::dynamic_pointer_cast<const ocio::FileTransform>(transform))
        throw Admission("external FileTransform requires a separately admitted closed resource runtime");
    if (auto group = std::dynamic_pointer_cast<const ocio::GroupTransform>(transform)) {
        for (int i = 0; i < group->getNumTransforms(); ++i) scan(group->getTransform(i), depth+1, nodes);
    }
}
static void closed(const ocio::ConstConfigRcPtr &config) {
    if (config->getNumEnvironmentVars() || config->getCurrentContext()->getNumStringVars())
        throw Admission("ambient configuration context is not an admitted closed program");
    unsigned nodes = 0;
    int count = config->getNumColorSpaces(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL);
    if (count > 512 || config->getNumLooks() > 512 || config->getNumViewTransforms() > 512 ||
        config->getNumNamedTransforms(ocio::NAMEDTRANSFORM_ALL) > 512)
        throw Admission("configuration catalogue exceeds traversal capacity");
    for (int i = 0; i < count; ++i) {
        auto space = config->getColorSpace(config->getColorSpaceNameByIndex(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL, i));
        scan(space->getTransform(ocio::COLORSPACE_DIR_TO_REFERENCE), 0, nodes);
        scan(space->getTransform(ocio::COLORSPACE_DIR_FROM_REFERENCE), 0, nodes);
    }
    for (int i = 0; i < config->getNumLooks(); ++i) {
        auto look = config->getLook(config->getLookNameByIndex(i));
        scan(look->getTransform(), 0, nodes);
        scan(look->getInverseTransform(), 0, nodes);
    }
    for (int i = 0; i < config->getNumViewTransforms(); ++i) {
        auto view = config->getViewTransform(config->getViewTransformNameByIndex(i));
        scan(view->getTransform(ocio::VIEWTRANSFORM_DIR_TO_REFERENCE), 0, nodes);
        scan(view->getTransform(ocio::VIEWTRANSFORM_DIR_FROM_REFERENCE), 0, nodes);
    }
    for (int i = 0; i < config->getNumNamedTransforms(ocio::NAMEDTRANSFORM_ALL); ++i) {
        auto named = config->getNamedTransform(config->getNamedTransformNameByIndex(ocio::NAMEDTRANSFORM_ALL, i));
        scan(named->getTransform(ocio::TRANSFORM_DIR_FORWARD), 0, nodes);
        scan(named->getTransform(ocio::TRANSFORM_DIR_INVERSE), 0, nodes);
    }
}
static void table(const Program *program, uint32_t index, Table *out) {
    if (index >= program->gpu->getNumTextures()) throw Admission("table index outside program");
    ocio::GpuShaderCreator::TextureType channel;
    ocio::GpuShaderCreator::TextureDimensions dimensions;
    ocio::Interpolation interpolation;
    program->gpu->getTexture(index, out->texture, out->sampler, out->width, out->height, channel, dimensions, interpolation);
    if (dimensions != ocio::GpuShaderCreator::TEXTURE_2D || !out->width || !out->height ||
        uint64_t(out->width)*out->height > 262144 ||
        (channel != ocio::GpuShaderCreator::TEXTURE_RED_CHANNEL && channel != ocio::GpuShaderCreator::TEXTURE_RGB_CHANNEL) ||
        (interpolation != ocio::INTERP_NEAREST && interpolation != ocio::INTERP_LINEAR))
        throw Admission("unsupported table dimensions, channels or interpolation");
    length(out->texture, 128); length(out->sampler, 128);
    out->channels = channel == ocio::GpuShaderCreator::TEXTURE_RED_CHANNEL ? 1 : 3;
    out->interpolation = interpolation == ocio::INTERP_NEAREST ? 0 : 1;
    out->binding = program->gpu->getTextureShaderBindingIndex(index);
    out->count = size_t(out->width)*out->height*out->channels;
    program->gpu->getTextureValues(index, out->values);
    if (!out->values) throw Admission("OCIO returned a null table");
}
MOTION_EXPORT bool motion_ocio_prepare(const char *bytes, size_t size, const char *from,
    const char *to, const char *view, Program **output, Error *error) noexcept {
    *output = nullptr;
    return checked(error, [&] {
        if (!size || size > 8*1024*1024) throw Admission("configuration bytes exceed capacity");
        length(from, 256); length(to, 256);
        if (view) length(view, 256);
        // No global environment mutation, fallback config or context expansion.
        for (const char *name : {"OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES"})
            if (std::getenv(name)) throw Admission("ambient OCIO catalogue override must be removed before preparing a closed program");
        std::istringstream stream(std::string(bytes, size));
        auto config = ocio::Config::CreateFromStream(stream);
        closed(config); // Before validate/getProcessor may resolve external files.
        config->validate();
        auto input = config->getColorSpace(from);
        auto outputSpace = view ? ocio::ConstColorSpaceRcPtr{} : config->getColorSpace(to);
        if ((input && input->isData()) || (outputSpace && outputSpace->isData()))
            throw Admission("native data spaces must bypass color processors");
        auto processor = view ? config->getProcessor(from, to, view, ocio::TRANSFORM_DIR_FORWARD) : config->getProcessor(from, to);
        if (processor->getProcessorMetadata()->getNumFiles()) throw Admission("processor references external files");
        auto program = std::make_unique<Program>();
        program->cpu = processor->getDefaultCPUProcessor();
        program->gpu = ocio::GpuShaderDesc::CreateShaderDesc();
        program->gpu->setLanguage(ocio::GPU_LANGUAGE_GLSL_VK_4_6);
        program->gpu->setAllowTexture1D(false);
        program->gpu->setTextureMaxWidth(4096);
        program->gpu->setFunctionName("motion_ocio_transform");
        program->gpu->setResourcePrefix("motion_ocio_");
        processor->getDefaultGPUProcessor()->extractGpuShaderInfo(program->gpu);
        if (program->gpu->getNum3DTextures() || program->gpu->getNumUniforms() || program->gpu->getNumTextures() > 4)
            throw Admission("3D tables, dynamic uniforms or excessive tables are not admitted");
        length(program->gpu->getShaderText(), 256*1024);
        for (unsigned i = 0; i < program->gpu->getNumTextures(); ++i) { Table info{}; table(program.get(), i, &info); }
        *output = program.release();
    });
}
MOTION_EXPORT bool motion_ocio_shader(const Program *program, const char **text, size_t *size,
    uint32_t *tables, Error *error) noexcept {
    return checked(error, [&] {
        *text = program->gpu->getShaderText();
        *size = length(*text, 256*1024);
        *tables = program->gpu->getNumTextures();
    });
}
MOTION_EXPORT bool motion_ocio_table(const Program *program, uint32_t index, Table *out, Error *error) noexcept {
    return checked(error, [&] { table(program, index, out); });
}
MOTION_EXPORT bool motion_ocio_apply_rgb(const Program *program, float *rgb, Error *error) noexcept {
    return checked(error, [&] { program->cpu->applyRGB(rgb); });
}
MOTION_EXPORT void motion_ocio_release(Program *program) noexcept { delete program; }
