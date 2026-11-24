// Independently authored comparison with OCIO's own Cube file reader. File IO
// belongs only to this qualification executable, never the installed bridge.
#include "bridge.cpp"
#include <array>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <locale>

static void require(bool value, const char *reason) {
    if (!value) throw std::runtime_error(reason);
}
static void quoted(std::ostream &out, const std::string &value) {
    out << '"';
    for (unsigned char c : value) {
        if (c=='"' || c=='\\') out << '\\' << c;
        else if (c=='\n') out << "\\n";
        else if (c=='\r') out << "\\r";
        else if (c=='\t') out << "\\t";
        else if (c<32) throw std::runtime_error("unexpected control character");
        else out << c;
    }
    out << '"';
}
static void pixel(std::ostream &out, const std::array<float,4> &value) {
    out << '[' << value[0] << ',' << value[1] << ',' << value[2] << ',' << value[3] << ']';
}
int main(int argc, char **argv) {
    try {
        require(argc==2,"one exclusive output path is required");
        const auto destination=std::filesystem::u8path(argv[1]);
        require(!std::filesystem::exists(destination),"resource fixture already exists");
        const auto directory=destination.parent_path()/"resource-oracle-files";
        require(std::filesystem::create_directory(directory),"oracle directory must be exclusive");
        std::filesystem::create_directory(directory/std::filesystem::u8path(u8"\u8d44\u6e90"));
        const std::array<std::array<float,4>,12> input{{
            {{-1,0.3f,0.8f,1}},{{3,0.2f,0.4f,0.5f}},{{0,0,0,0}},{{1,1,1,1}},
            {{0.1f,0.3f,0.7f,0.001f}},{{0.7f,0.1f,0.3f,0.2f}},{{0.3f,0.7f,0.1f,0.8f}},
            {{0.15f,0.8f,0.4f,1}},{{0.8f,0.4f,0.15f,1}},{{0.4f,0.15f,0.8f,1}},
            {{0.5f,0.5f,0.5f,1}},{{0.237f,0.619f,0.881f,1}}
        }};
        std::ostringstream output;
        output.imbue(std::locale::classic());
        output<<std::setprecision(std::numeric_limits<float>::max_digits10);
        output<<"{\"version\":1,\"runtime\":\"2.6.0\",\"cases\":[";
        unsigned cases=0, pixels=0;
        for (unsigned variant=0;variant<5;++variant) {
            // Distinct oracle paths prevent OCIO's process-wide file cache
            // from reusing a different variant's earlier table.
            const std::string name=std::string(u8"\u8d44\u6e90/\u989c\u8272-")+std::to_string(variant)+".cube";
            const auto file_path=directory/std::filesystem::u8path(name);
            const bool one=variant<2, inverse=variant==1;
            const unsigned edge=one?16:5;
            const auto interpolation=variant==2?ocio::INTERP_DEFAULT:variant==4?ocio::INTERP_TETRAHEDRAL:ocio::INTERP_LINEAR;
            std::vector<float> values;
            std::ostringstream cube;
            cube.imbue(std::locale::classic());
            cube<<std::setprecision(std::numeric_limits<float>::max_digits10);
            cube<<(one?"LUT_1D_SIZE ":"LUT_3D_SIZE ")<<edge<<"\nDOMAIN_MIN -0.25 -0.5 -0.75\nDOMAIN_MAX 1.5 1.75 2\n";
            for(unsigned b=0;b<(one?1:edge);++b) for(unsigned g=0;g<(one?1:edge);++g) for(unsigned r=0;r<edge;++r) {
                const float x=float(r)/(edge-1), y=one?x:float(g)/(edge-1), z=one?x:float(b)/(edge-1);
                const std::array<float,3> value{{x*x+0.2f*y*z-0.1f,y*y+0.3f*x*z+0.02f,z*z+0.25f*x*y}};
                values.insert(values.end(),value.begin(),value.end());
                cube<<value[0]<<' '<<value[1]<<' '<<value[2]<<'\n';
            }
            {std::ofstream file(file_path,std::ios::binary);file<<cube.str();require(file.good(),"oracle Cube write failed");}
            auto config=ocio::Config::Create();
            config->setMajorVersion(2);config->setMinorVersion(1);
            for(const char *space_name:{"Input","Working","Output"}) {
                auto space=ocio::ColorSpace::Create(ocio::REFERENCE_SPACE_SCENE);
                space->setName(space_name);space->setEncoding("scene-linear");
                if(std::string(space_name)!="Working") {
                    auto file=ocio::FileTransform::Create();file->setSrc(name.c_str());
                    file->setInterpolation(interpolation);
                    file->setDirection(inverse?ocio::TRANSFORM_DIR_INVERSE:ocio::TRANSFORM_DIR_FORWARD);
                    space->setTransform(file,std::string(space_name)=="Input"?ocio::COLORSPACE_DIR_TO_REFERENCE:ocio::COLORSPACE_DIR_FROM_REFERENCE);
                }
                config->addColorSpace(space);
            }
            config->setRole("default","Working");config->setRole("scene_linear","Working");config->setRole("color_picking","Input");
            config->addDisplayView("Monitor","View","Output","");
            auto rules=ocio::FileRules::Create();rules->setColorSpace(0,"Working");config->setFileRules(rules);
            std::ostringstream serialized;config->serialize(serialized);
            const auto bytes=serialized.str();
            config->setWorkingDir(directory.u8string().c_str());config->addSearchPath(".");config->validate();
            auto oracle=config->getProcessor("Input","Working")->getDefaultCPUProcessor();
            Resource item{name.c_str(),one?1u:3u,edge,{-0.25f,-0.5f,-0.75f},{1.5f,1.75f,2.f},values.data(),values.size()/3};
            Program *raw=nullptr;Error error{};
            if(!motion_ocio_prepare(bytes.data(),bytes.size(),"Input","Working",nullptr,&item,1,&raw,&error))
                throw std::runtime_error(std::string("resource prepare: ")+error.message);
            std::unique_ptr<Program> program(raw);
            if(cases++)output<<',';
            output<<"{\"config\":";quoted(output,bytes);
            output<<",\"name\":";quoted(output,name);
            output<<",\"cube\":";quoted(output,cube.str());
            output<<",\"input\":[";
            for(size_t p=0;p<input.size();++p){if(p)output<<',';pixel(output,input[p]);}
            output<<"],\"expected\":[";
            for(size_t p=0;p<input.size();++p) {
                auto expected=input[p],actual=input[p];oracle->applyRGB(expected.data());
                require(motion_ocio_apply_rgb(program.get(),actual.data(),&error),"closed CPU apply failed");
                for(unsigned c=0;c<3;++c) if(std::abs(expected[c]-actual[c])>0.0001f+0.0002f*std::abs(expected[c])) {
                    std::cerr<<"variant="<<variant<<" pixel="<<p<<" channel="<<c<<" expected="<<expected[c]<<" actual="<<actual[c]<<'\n';
                    throw std::runtime_error("closed/file CPU mismatch");
                }
                if(p)output<<',';pixel(output,expected);++pixels;
            }
            output<<"],\"composite\":[";
            // Independent full-chain card: input LUT, straight source-over
            // composition, then output LUT. This never uses the Rust adapter.
            const std::array<std::array<float,4>,2> fronts{{
                {{0.2f,0.8f,0.4f,0.6f}},
                {{16384.f/65535.f,32768.f/65535.f,8192.f/65535.f,32768.f/65535.f}}
            }};
            for(size_t f=0;f<fronts.size();++f) {
                auto front=fronts[f];std::array<float,4> background{{0.08f,0.1f,0.2f,0.25f}};
                oracle->applyRGB(front.data());oracle->applyRGB(background.data());
                const auto alpha=front[3]+background[3]*(1-front[3]);
                for(unsigned c=0;c<3;++c)front[c]=(front[c]*front[3]+background[c]*background[3]*(1-front[3]))/alpha;
                front[3]=alpha;oracle->applyRGB(front.data());
                if(f)output<<',';pixel(output,front);
            }
            output<<"]}";
            require(std::filesystem::remove(file_path),"oracle source cleanup failed");
            // Both CPU and extracted GPU tables must own all data at this point.
            auto after=input[0];require(motion_ocio_apply_rgb(program.get(),after.data(),&error),"CPU retained a file dependency");
            raw=nullptr;
            require(!motion_ocio_prepare(bytes.data(),bytes.size(),"Working","Working",nullptr,nullptr,0,&raw,&error),"unselected undeclared FileTransform must fail");
            require(!raw&&error.kind==1,"failed resource preparation published a program");
            auto bad=config->createEditableCopy();bad->clearSearchPaths();bad->setWorkingDir("");
            auto space=bad->getColorSpace("Input")->createEditableCopy();
            auto nearest=ocio::FileTransform::Create();nearest->setSrc(name.c_str());nearest->setInterpolation(ocio::INTERP_NEAREST);
            space->setTransform(nearest,ocio::COLORSPACE_DIR_TO_REFERENCE);bad->addColorSpace(space);
            std::ostringstream rejected;bad->serialize(rejected);const auto rejected_bytes=rejected.str();
            require(!motion_ocio_prepare(rejected_bytes.data(),rejected_bytes.size(),"Working","Working",nullptr,&item,1,&raw,&error),"native nearest normalization must be refused");
            require(!raw&&error.kind==1&&std::strstr(error.message,"nearest resource interpolation"),"nearest refusal lost its cause or published a program");
            Resource invalid=item;invalid.size=0;raw=nullptr;
            require(!motion_ocio_prepare(bytes.data(),bytes.size(),"Input","Working",nullptr,&invalid,1,&raw,&error),"invalid resource shape must fail");
            require(!raw&&error.kind==1,"invalid shape published a partial program");
            auto admitted=resources(&item,1);
            size_t expanded=64*1024*1024;
            auto bounded=ocio::FileTransform::Create();bounded->setSrc(name.c_str());bounded->setInterpolation(ocio::INTERP_LINEAR);
            require(!checked(&error,[&]{file_transform(bounded,admitted,expanded);}),"expanded resource budget must fail before allocation");
            require(error.kind==1&&expanded==64*1024*1024&&std::strstr(error.message,"expanded resource transforms"),"resource budget changed or lost its cause");
        }
        require(std::filesystem::remove(directory/std::filesystem::u8path(u8"\u8d44\u6e90")),"oracle subdirectory cleanup failed");
        require(std::filesystem::remove(directory),"oracle directory cleanup failed");
        output<<"]}";
        std::ofstream file(destination,std::ios::binary);file<<output.str();require(file.good(),"resource fixture publication failed");
        std::cout<<"native_resources cases="<<cases<<" file_cpu_pixels="<<pixels<<" unselected_missing=refused source_removed=true\n";
        return 0;
    }catch(const std::exception &error){std::cerr<<"native resource qualification failed: "<<error.what()<<'\n';return 1;}
    catch(...){std::cerr<<"native resource qualification failed: unknown exception\n";return 1;}
}
