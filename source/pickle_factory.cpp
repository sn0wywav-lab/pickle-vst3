#include "pickle_processor.h"
#include "pickle_controller.h"
#include "public.sdk/source/main/pluginfactory.h"

namespace Pickle {

//--- Factory ------------------------------------------------------------------
using namespace Steinberg::Vst;

extern "C" {
EXPORT_API Steinberg::IPluginFactory* PLUGIN_API GetPluginFactory()
{
    static Steinberg::Vst::PluginFactory factory;
    
    // Register the PICKLE plugin
    Steinberg::PFactoryInfo factoryInfo(
        "Pickle Audio",
        "https://pickleaudio.com",
        "info@pickleaudio.com",
        Steinberg::Vst::kDefaultFactoryFlags);
    
    factory.setFactoryInfo(&factoryInfo);
    
    // Register processor class
    Steinberg::PClassInfo classInfo;
    classInfo.cid = PickleProcessor::uid;
    classInfo.category = kAudioEffectClass;
    classInfo.subCategory = kDspProcessorSubCategory;
    classInfo.name = "PICKLE";
    classInfo.classFlags = 0;
    classInfo.cardinality = Steinberg::Vst::kUnlimited;
    classInfo.vendor = "Pickle Audio";
    classInfo.version = "1.0.0";
    classInfo.sdkVersion = Steinberg::Vst::VST3_SDK_VERSION_STR;
    
    factory.registerClass(classInfo, PickleProcessor::createInstance, PickleController::createInstance);
    
    return &factory;
}
}

} // namespace Pickle
