// Independently authored qualification of the exact production table ABI.
// Inline OCIO transforms provide CPU oracles without adding a FileTransform
// bypass, an external-resource loader, or test hooks to the shipped bridge.
#include "bridge.cpp"
#include <array>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <locale>

static void require(bool condition, const char *reason) {
    if (!condition) throw std::runtime_error(reason);
}
static void quoted(std::ostream &out, const std::string &value) {
    out << '"';
    for (unsigned char c : value) {
        if (c == '"' || c == '\\') out << '\\' << c;
        else if (c == '\n') out << "\\n";
        else if (c == '\r') out << "\\r";
        else if (c == '\t') out << "\\t";
        else if (c < 32) throw std::runtime_error("unexpected shader control character");
        else out << c;
    }
    out << '"';
}
static void vector(std::ostream &out, const std::array<float,4> &value) {
    out << '[' << value[0] << ',' << value[1] << ',' << value[2] << ',' << value[3] << ']';
}
static Program prepare(unsigned edge, bool tetrahedral, bool mixed) {
    auto lut = ocio::Lut3DTransform::Create(edge);
    lut->setInterpolation(tetrahedral ? ocio::INTERP_TETRAHEDRAL : ocio::INTERP_LINEAR);
    for (unsigned r = 0; r < edge; ++r) for (unsigned g = 0; g < edge; ++g) for (unsigned b = 0; b < edge; ++b) {
        const float x = float(r)/float(edge-1), y = float(g)/float(edge-1), z = float(b)/float(edge-1);
        lut->setValue(r,g,b, x*x+0.2f*y*z-0.1f, y*y+0.3f*x*z+0.02f, z*z+0.25f*x*y);
    }
    auto group = ocio::GroupTransform::Create();
    if (mixed) {
        auto curve = ocio::Lut1DTransform::Create(4, false);
        curve->setInterpolation(ocio::INTERP_LINEAR);
        for (unsigned i=0; i<4; ++i) {
            const float x = float(i)/3.f;
            curve->setValue(i, x*x, x*0.8f, x*0.6f+0.1f);
        }
        group->appendTransform(curve);
    }
    group->appendTransform(lut);
    auto processor = ocio::Config::CreateRaw()->getProcessor(group);
    Program program;
    program.cpu = processor->getDefaultCPUProcessor();
    program.gpu = ocio::GpuShaderDesc::CreateShaderDesc();
    program.gpu->setLanguage(ocio::GPU_LANGUAGE_GLSL_VK_4_6);
    program.gpu->setAllowTexture1D(false);
    program.gpu->setFunctionName("motion_ocio_transform");
    program.gpu->setResourcePrefix("motion_ocio_");
    processor->getDefaultGPUProcessor()->extractGpuShaderInfo(program.gpu);
    return program;
}
int main(int argc, char **argv) {
    try {
        require(argc == 2, "one exclusive output path is required");
        require(!std::ifstream(argv[1]).good(), "cube fixture destination already exists");
        const std::array<std::array<float,4>,12> inputs{{
            {{-0.2f,0.3f,0.8f,1.f}}, {{1.8f,0.2f,0.4f,0.5f}}, {{0,0,0,0}},
            {{1,1,1,1}}, {{0.1f,0.3f,0.7f,0.001f}}, {{0.7f,0.1f,0.3f,0.2f}},
            {{0.3f,0.7f,0.1f,0.8f}}, {{0.15f,0.8f,0.4f,1}}, {{0.8f,0.4f,0.15f,1}},
            {{0.4f,0.15f,0.8f,1}}, {{0.5f,0.5f,0.5f,1}}, {{0.237f,0.619f,0.881f,1}}
        }};
        std::ostringstream output;
        output.imbue(std::locale::classic());
        output << std::setprecision(std::numeric_limits<float>::max_digits10);
        output << "{\"version\":1,\"runtime\":\"2.6.0\",\"cases\":[";
        bool first_case=true;
        for (unsigned edge : {2u,5u}) for (bool tetrahedral : {false,true}) for (bool mixed : {false,true}) {
            auto program = prepare(edge,tetrahedral,mixed);
            require(program.gpu->getNum3DTextures() == 1, "native processor must contain one actual cube");
            require(program.gpu->getNumTextures() == (mixed ? 1u : 0u), "mixed program must retain its actual 2D table");
            const char *shader=nullptr; size_t length=0; uint32_t count=0; Error error{};
            require(motion_ocio_shader(&program,&shader,&length,&count,&error), "production shader ABI failed");
            if (!first_case) output << ',';
            first_case=false;
            output << "{\"edge\":" << edge << ",\"tetrahedral\":" << (tetrahedral?"true":"false")
                   << ",\"mixed\":" << (mixed?"true":"false") << ",\"shader\":";
            quoted(output,std::string(shader,length));
            output << ",\"tables\":[";
            for (uint32_t index=0; index<count; ++index) {
                Table table{};
                require(motion_ocio_table(&program,index,&table,&error), "production table ABI failed");
                require(table.values && table.count, "borrowed table values missing");
                if (index) output << ',';
                output << "{\"texture_name\":"; quoted(output,table.texture);
                output << ",\"sampler_name\":"; quoted(output,table.sampler);
                output << ",\"binding\":" << table.binding << ",\"width\":" << table.width
                       << ",\"height\":" << table.height << ",\"depth\":" << table.depth
                       << ",\"channels\":" << table.channels << ",\"interpolation\":\""
                       << (table.interpolation?"linear":"nearest") << "\",\"values\":[";
                for (size_t v=0; v<table.count; ++v) { if (v) output << ','; output << table.values[v]; }
                output << "]}";
            }
            Table invalid{}; invalid.depth=99;
            require(!motion_ocio_table(&program,count,&invalid,&error), "invalid table index must fail");
            require(error.kind==1 && !invalid.values && !invalid.depth, "table failure must preserve cause without publishing partial output");
            output << "],\"input\":[";
            for (size_t i=0; i<inputs.size(); ++i) { if (i) output << ','; vector(output,inputs[i]); }
            output << "],\"expected\":[";
            for (size_t i=0; i<inputs.size(); ++i) {
                auto expected=inputs[i];
                // Independent OCIO CPU processor, never the shader/sample adapter.
                program.cpu->applyRGB(expected.data());
                if (i) output << ',';
                vector(output,expected);
            }
            output << "]}";
        }
        output << "]}\n";
        auto excessive=prepare(65,false,false);
        Table rejected{}; Error error{};
        require(!motion_ocio_table(&excessive,0,&rejected,&error), "65-cube must exceed the existing 262144-texel limit");
        require(error.kind==1 && !rejected.values && !rejected.depth, "capacity error must publish no table");
        require(!motion_ocio_table(nullptr,0,&rejected,&error) && error.kind==1, "null table owner must return its original bounded error");
        std::ofstream file(argv[1],std::ios::binary|std::ios::out);
        file << output.str(); file.flush();
        require(file.good(), "could not write complete native cube fixture");
        std::cout << "native_cubes cases=8 cpu_pixels=96 table_abi=passed capacity=passed partial_publication=false\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "native cube qualification failed: " << error.what() << '\n';
        return 1;
    }
}
