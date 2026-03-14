#include "pickle_factory.cpp"

//--- Entry point -------------------------------------------------------------
extern "C" {
EXPORT_API Steinberg::IPluginFactory* PLUGIN_API GetPluginFactory()
{
    // Get the global plugin factory
    return Pickle::GetPluginFactory();
}
}
