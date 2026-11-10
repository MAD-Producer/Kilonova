// Independently authored C ABI. No C++ exception or allocator ownership crosses it.
#include <OpenColorIO/OpenColorIO.h>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <memory>
#include <cmath>
#include <map>
#include <limits>
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
    uint32_t binding, width, height, depth, channels, interpolation;
    const float *values;
    size_t count;
};
struct Resource {
    const char *name;
    uint32_t kind, size;
    float min[3], max[3];
    const float *values;
    size_t count;
};
using Resources = std::map<std::string, const Resource *>;
struct CatalogueEntry {
    uint32_t kind, reference, data;
    std::string name, value;
};
struct CatalogueRow {
    uint32_t kind, reference, data;
    const char *name, *value;
};
struct Program {
    ocio::ConstCPUProcessorRcPtr cpu;
    ocio::GpuShaderDescRcPtr gpu;
    std::vector<CatalogueEntry> catalogue;
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
static Resources resources(const Resource *items, size_t count) {
    if (count > 128 || (count && !items)) throw Admission("resource array exceeds admission capacity");
    Resources result;
    size_t retained = 0;
    for (size_t i = 0; i < count; ++i) {
        const auto &item = items[i];
        const auto name_size = length(item.name, 256);
        if (!name_size || !result.emplace(item.name, &item).second)
            throw Admission("empty or duplicate resource name");
        const size_t expected = item.kind == 1 ? item.size : size_t(item.size)*item.size*item.size;
        if ((item.kind != 1 && item.kind != 3) || item.size < 2 ||
            item.size > (item.kind == 1 ? 65536u : 64u) || item.count != expected || !item.values)
            throw Admission("resource table shape exceeds admission capacity");
        retained += item.count * 3 * sizeof(float);
        if (retained > 64*1024*1024) throw Admission("numeric resource bundle exceeds admission capacity");
        for (unsigned c=0; c<3; ++c) {
            const float span = item.max[c]-item.min[c];
            if (!std::isfinite(item.min[c]) || !std::isfinite(item.max[c]) ||
                !std::isfinite(span) || span < std::numeric_limits<float>::min())
                throw Admission("resource domain requires finite increasing normal-width intervals");
        }
        for (size_t v=0; v<item.count*3; ++v)
            if (!std::isfinite(item.values[v])) throw Admission("nonfinite numeric resource sample");
    }
    return result;
}
static ocio::TransformRcPtr file_transform(const ocio::ConstFileTransformRcPtr &file, const Resources &items, size_t &expanded) {
    const std::string name(file->getSrc(), length(file->getSrc(), 256));
    const auto found = items.find(name);
    if (found == items.end()) throw Admission("external FileTransform has no verified resource: " + name);
    if (length(file->getCCCId(), 256) || file->getCDLStyle() != ocio::CDL_NO_CLAMP)
        throw Admission("Cube FileTransform cannot select CDL options: " + name);
    const auto &item = *found->second;
    const auto bytes = item.count * 3 * sizeof(float);
    if (bytes > 64*1024*1024 - expanded)
        throw Admission("expanded resource transforms exceed numeric byte capacity: " + name);
    expanded += bytes;
    const auto interpolation = file->getInterpolation();
    // OCIO 2.6 normalizes NEAREST to linear/trilinear for file LUTs. Do not
    // accept a declared nearest operation and quietly produce different math.
    if (interpolation == ocio::INTERP_NEAREST)
        throw Admission("native OCIO does not preserve nearest resource interpolation: " + name);
    if (interpolation != ocio::INTERP_DEFAULT && interpolation != ocio::INTERP_BEST &&
        interpolation != ocio::INTERP_LINEAR &&
        !(item.kind == 3 && interpolation == ocio::INTERP_TETRAHEDRAL))
        throw Admission("unsupported explicit resource interpolation: " + name);
    auto group = ocio::GroupTransform::Create();
    group->setDirection(file->getDirection());
    auto domain = ocio::MatrixTransform::Create();
    double matrix[16] = {}, offset[4] = {};
    matrix[15] = 1;
    for (unsigned c=0; c<3; ++c) {
        matrix[c*5] = 1.0 / (double(item.max[c])-double(item.min[c]));
        offset[c] = -double(item.min[c]) * matrix[c*5];
    }
    domain->setMatrix(matrix); domain->setOffset(offset);
    group->appendTransform(domain);
    if (item.kind == 1) {
        auto lut = ocio::Lut1DTransform::Create(item.size, false);
        lut->setInterpolation(interpolation);
        for (unsigned i=0; i<item.size; ++i)
            lut->setValue(i, item.values[i*3], item.values[i*3+1], item.values[i*3+2]);
        group->appendTransform(lut);
    } else {
        auto lut = ocio::Lut3DTransform::Create(item.size);
        lut->setInterpolation(interpolation);
        for (unsigned b=0; b<item.size; ++b) for (unsigned g=0; g<item.size; ++g) for (unsigned r=0; r<item.size; ++r) {
            const auto index = size_t(r+item.size*(g+item.size*b))*3;
            lut->setValue(r,g,b,item.values[index],item.values[index+1],item.values[index+2]);
        }
        group->appendTransform(lut);
    }
    return group;
}
static ocio::TransformRcPtr scan(const ocio::ConstTransformRcPtr &transform, const Resources &items, unsigned depth, unsigned &nodes, size_t &expanded) {
    if (!transform) return {};
    if (depth > 32 || ++nodes > 8192) throw Admission("configuration transform traversal exceeds capacity");
    if (auto file = std::dynamic_pointer_cast<const ocio::FileTransform>(transform))
        return file_transform(file, items, expanded);
    auto copy = transform->createEditableCopy();
    if (auto group = std::dynamic_pointer_cast<ocio::GroupTransform>(copy)) {
        for (int i = 0; i < group->getNumTransforms(); ++i)
            group->getTransform(i) = scan(group->getTransform(i), items, depth+1, nodes, expanded);
    }
    return copy;
}
static ocio::ConfigRcPtr closed(const ocio::ConstConfigRcPtr &config, const Resources &items) {
    if (config->getNumEnvironmentVars() || config->getCurrentContext()->getNumStringVars())
        throw Admission("ambient configuration context is not an admitted closed program");
    if (!items.empty()) for (int i=0; i<config->getNumSearchPaths(); ++i)
        if (length(config->getSearchPath(i), 256))
            throw Admission("resource configuration search paths are not admitted");
    auto result = config->createEditableCopy();
    unsigned nodes = 0;
    size_t expanded = 0;
    int count = config->getNumColorSpaces(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL);
    if (count > 512 || config->getNumLooks() > 512 || config->getNumViewTransforms() > 512 ||
        config->getNumNamedTransforms(ocio::NAMEDTRANSFORM_ALL) > 512)
        throw Admission("configuration catalogue exceeds traversal capacity");
    for (int i = 0; i < count; ++i) {
        auto space = config->getColorSpace(config->getColorSpaceNameByIndex(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL, i));
        auto edited = space->createEditableCopy();
        edited->setTransform(scan(space->getTransform(ocio::COLORSPACE_DIR_TO_REFERENCE), items, 0, nodes, expanded), ocio::COLORSPACE_DIR_TO_REFERENCE);
        edited->setTransform(scan(space->getTransform(ocio::COLORSPACE_DIR_FROM_REFERENCE), items, 0, nodes, expanded), ocio::COLORSPACE_DIR_FROM_REFERENCE);
        result->addColorSpace(edited);
    }
    for (int i = 0; i < config->getNumLooks(); ++i) {
        auto look = config->getLook(config->getLookNameByIndex(i));
        auto edited = look->createEditableCopy();
        // OCIO Look setters require a non-null transform, unlike ColorSpace
        // and NamedTransform. Keep absent directions absent in the copy.
        if (auto transform = scan(look->getTransform(), items, 0, nodes, expanded)) edited->setTransform(transform);
        if (auto transform = scan(look->getInverseTransform(), items, 0, nodes, expanded)) edited->setInverseTransform(transform);
        result->addLook(edited);
    }
    for (int i = 0; i < config->getNumViewTransforms(); ++i) {
        auto view = config->getViewTransform(config->getViewTransformNameByIndex(i));
        auto edited = view->createEditableCopy();
        edited->setTransform(scan(view->getTransform(ocio::VIEWTRANSFORM_DIR_TO_REFERENCE), items, 0, nodes, expanded), ocio::VIEWTRANSFORM_DIR_TO_REFERENCE);
        edited->setTransform(scan(view->getTransform(ocio::VIEWTRANSFORM_DIR_FROM_REFERENCE), items, 0, nodes, expanded), ocio::VIEWTRANSFORM_DIR_FROM_REFERENCE);
        result->addViewTransform(edited);
    }
    for (int i = 0; i < config->getNumNamedTransforms(ocio::NAMEDTRANSFORM_ALL); ++i) {
        auto named = config->getNamedTransform(config->getNamedTransformNameByIndex(ocio::NAMEDTRANSFORM_ALL, i));
        auto edited = named->createEditableCopy();
        edited->setTransform(scan(named->getTransform(ocio::TRANSFORM_DIR_FORWARD), items, 0, nodes, expanded), ocio::TRANSFORM_DIR_FORWARD);
        edited->setTransform(scan(named->getTransform(ocio::TRANSFORM_DIR_INVERSE), items, 0, nodes, expanded), ocio::TRANSFORM_DIR_INVERSE);
        result->addNamedTransform(edited);
    }
    return result;
}
static void table(const Program *program, uint32_t index, Table *out) {
    if (!program || !program->gpu) throw Admission("table program is missing");
    const auto planes = program->gpu->getNumTextures();
    if (index >= planes + program->gpu->getNum3DTextures()) throw Admission("table index outside program");
    if (index >= planes) {
        const auto cube = index - planes;
        ocio::Interpolation interpolation;
        program->gpu->get3DTexture(cube, out->texture, out->sampler, out->depth, interpolation);
        if (out->depth < 2 || out->depth > 64 ||
            (interpolation != ocio::INTERP_NEAREST && interpolation != ocio::INTERP_LINEAR))
            throw Admission("unsupported 3D table edge or sampler interpolation");
        length(out->texture, 128); length(out->sampler, 128);
        out->width = out->height = out->depth;
        out->channels = 3;
        out->interpolation = interpolation == ocio::INTERP_NEAREST ? 0 : 1;
        out->binding = program->gpu->get3DTextureShaderBindingIndex(cube);
        out->count = size_t(out->depth)*out->depth*out->depth*3;
        program->gpu->get3DTextureValues(cube, out->values);
        if (!out->values) throw Admission("OCIO returned a null 3D table");
        return;
    }
    out->depth = 1;
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
// Snapshot the configuration's declared canonical spaces and active display/view
// catalogue. Active lists are part of the admitted config bytes; environment
// overrides were refused above. Ordering is not a colour-space identity.
static std::string builtin_identity(const ocio::ConstConfigRcPtr &config) {
    const auto &registry = ocio::BuiltinConfigRegistry::Get();
    if (registry.getNumBuiltinConfigs() > 64)
        throw Admission("native builtin registry exceeds capacity");
    for (size_t i = 0; i < registry.getNumBuiltinConfigs(); ++i) {
        const char *id = registry.getBuiltinConfigName(i);
        length(id, 256);
        // Only exact, versioned registry names; no default/latest URI aliases.
        if (std::strcmp(config->getName(), id) != 0) continue;
        auto builtin = ocio::Config::CreateFromBuiltinConfig(id);
        builtin->validate();
        std::ostringstream actual, expected;
        // A failed serialization must preserve its exception, never compare
        // equal partial strings and accidentally publish a builtin identity.
        actual.exceptions(std::ios::badbit | std::ios::failbit);
        expected.exceptions(std::ios::badbit | std::ios::failbit);
        config->serialize(actual);
        builtin->serialize(expected);
        if (actual.tellp() > 8*1024*1024 || expected.tellp() > 8*1024*1024)
            throw Admission("native builtin serialization exceeds capacity");
        // The name alone and the currently selected processor are insufficient:
        // compare the entire canonical configuration before resource rewriting.
        return actual.str() == expected.str() ? std::string(id) : std::string{};
    }
    return {};
}
static std::vector<CatalogueEntry> catalogue(const ocio::ConstConfigRcPtr &config,
    const std::string &builtin = {}) {
    std::vector<CatalogueEntry> rows;
    auto add = [&](uint32_t kind, const char *name, const char *value, uint32_t reference = 0, uint32_t data = 0) {
        if (!length(name, 256)) throw Admission("native catalogue name is empty");
        length(value, 256);
        if (rows.size() >= 2721) throw Admission("native catalogue row capacity exceeded");
        rows.push_back({kind, reference, data, name, value});
    };
    const int spaces = config->getNumColorSpaces(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL);
    const int roles = config->getNumRoles(), displays = config->getNumDisplays();
    if (spaces < 1 || spaces > 512 || roles < 0 || roles > 128 || displays < 0 || displays > 32)
        throw Admission("native catalogue spaces, roles or displays exceed capacity");
    for (int i = 0; i < spaces; ++i) {
        auto space = config->getColorSpace(config->getColorSpaceNameByIndex(ocio::SEARCH_REFERENCE_SPACE_ALL, ocio::COLORSPACE_ALL, i));
        const auto reference = space->getReferenceSpaceType();
        if (reference != ocio::REFERENCE_SPACE_SCENE && reference != ocio::REFERENCE_SPACE_DISPLAY)
            throw Admission("native catalogue reference space is unsupported");
        add(0, space->getName(), space->getEncoding(), reference == ocio::REFERENCE_SPACE_DISPLAY ? 1 : 0, space->isData() ? 1 : 0);
    }
    for (int i = 0; i < roles; ++i) {
        auto space = config->getColorSpace(config->getRoleColorSpace(i));
        if (!space) throw Admission("native catalogue role has no resolved space");
        add(1, config->getRoleName(i), space->getName());
    }
    for (int i = 0; i < displays; ++i) {
        const char *display = config->getDisplay(i);
        add(2, display, "");
        const int views = config->getNumViews(display);
        if (views < 1 || views > 64) throw Admission("native catalogue views exceed capacity");
        for (int j = 0; j < views; ++j) add(3, display, config->getView(display, j));
    }
    if (!builtin.empty()) add(4, "builtin", builtin.c_str());
    return rows;
}
MOTION_EXPORT bool motion_ocio_prepare(const char *bytes, size_t size, const char *from,
    const char *to, const char *view, const Resource *items, size_t count, Program **output, Error *error) noexcept {
    *output = nullptr;
    return checked(error, [&] {
        if (!size || size > 8*1024*1024) throw Admission("configuration bytes exceed capacity");
        length(from, 256); length(to, 256);
        if (view) length(view, 256);
        // No global environment mutation, fallback config or context expansion.
        for (const char *name : {"OCIO_ACTIVE_DISPLAYS", "OCIO_ACTIVE_VIEWS", "OCIO_INACTIVE_COLORSPACES"})
            if (std::getenv(name)) throw Admission("ambient OCIO catalogue override must be removed before preparing a closed program");
        std::istringstream stream(std::string(bytes, size));
        auto parsed = ocio::Config::CreateFromStream(stream);
        const auto builtin = builtin_identity(parsed);
        auto config = closed(parsed, resources(items, count));
        // All FileTransforms are replaced from verified numeric resources before
        // validate/getProcessor. OCIO never resolves a path or caches a file.
        config->validate();
        auto declared = catalogue(config, builtin);
        auto input = config->getColorSpace(from);
        auto outputSpace = view ? ocio::ConstColorSpaceRcPtr{} : config->getColorSpace(to);
        if ((input && input->isData()) || (outputSpace && outputSpace->isData()))
            throw Admission("native data spaces must bypass color processors");
        auto processor = view ? config->getProcessor(from, to, view, ocio::TRANSFORM_DIR_FORWARD) : config->getProcessor(from, to);
        if (processor->getProcessorMetadata()->getNumFiles()) throw Admission("processor references external files");
        auto program = std::make_unique<Program>();
        program->catalogue = std::move(declared);
        program->cpu = processor->getDefaultCPUProcessor();
        program->gpu = ocio::GpuShaderDesc::CreateShaderDesc();
        program->gpu->setLanguage(ocio::GPU_LANGUAGE_GLSL_VK_4_6);
        program->gpu->setAllowTexture1D(false);
        program->gpu->setTextureMaxWidth(4096);
        program->gpu->setFunctionName("motion_ocio_transform");
        program->gpu->setResourcePrefix("motion_ocio_");
        processor->getDefaultGPUProcessor()->extractGpuShaderInfo(program->gpu);
        if (program->gpu->getNumUniforms() || program->gpu->getNumTextures() + program->gpu->getNum3DTextures() > 4)
            throw Admission("dynamic uniforms or excessive tables are not admitted");
        length(program->gpu->getShaderText(), 256*1024);
        for (unsigned i = 0; i < program->gpu->getNumTextures() + program->gpu->getNum3DTextures(); ++i) { Table info{}; table(program.get(), i, &info); }
        *output = program.release();
    });
}
MOTION_EXPORT bool motion_ocio_catalogue_row(const Program *program, uint32_t index,
    CatalogueRow *out, uint32_t *count, Error *error) noexcept {
    *out = {}; *count = 0;
    return checked(error, [&] {
        if (!program || index >= program->catalogue.size()) throw Admission("native catalogue row index outside program");
        const auto &row = program->catalogue[index];
        *out = {row.kind, row.reference, row.data, row.name.c_str(), row.value.c_str()};
        *count = static_cast<uint32_t>(program->catalogue.size());
    });
}
MOTION_EXPORT bool motion_ocio_shader(const Program *program, const char **text, size_t *size,
    uint32_t *tables, Error *error) noexcept {
    return checked(error, [&] {
        *text = program->gpu->getShaderText();
        *size = length(*text, 256*1024);
        *tables = program->gpu->getNumTextures() + program->gpu->getNum3DTextures();
    });
}
MOTION_EXPORT bool motion_ocio_table(const Program *program, uint32_t index, Table *out, Error *error) noexcept {
    *out = {};
    return checked(error, [&] { Table candidate{}; table(program, index, &candidate); *out = candidate; });
}
MOTION_EXPORT bool motion_ocio_apply_rgb(const Program *program, float *rgb, Error *error) noexcept {
    return checked(error, [&] { program->cpu->applyRGB(rgb); });
}
MOTION_EXPORT void motion_ocio_release(Program *program) noexcept { delete program; }
