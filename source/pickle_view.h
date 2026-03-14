#pragma once

#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/base/funknown.h"
#include <vector>

namespace Pickle {

class PickleView : public Steinberg::Vst::IPlugView
{
public:
    PickleView(Steinberg::Vst::IEditController* controller);
    ~PickleView() override = default;

    //--- IPlugView -----------------------------------------------------------
    Steinberg::tresult PLUGIN_API attached(void* parent, Steinberg::FIDString type) override;
    Steinberg::tresult PLUGIN_API removed() override;
    Steinberg::tresult PLUGIN_API onSize(Steinberg::ViewRect* newSize) override;
    Steinberg::tresult PLUGIN_API getSize(Steinberg::ViewRect* size) override;
    Steinberg::tresult PLUGIN_API canResize() override;
    Steinberg::tresult PLUGIN_API checkSizeConstraint(Steinberg::ViewRect* rect) override;

    //--- IPlugViewContentScaleSupport ----------------------------------------
    Steinberg::tresult PLUGIN_API setContentScaleFactor(Steinberg::TScalar factor) override;

    //--- FUnknown ------------------------------------------------------------
    Steinberg::tresult PLUGIN_API queryInterface(const Steinberg::TUID iid, void** obj) override;
    Steinberg::uint32 PLUGIN_API addRef() override;
    Steinberg::uint32 PLUGIN_API release() override;

private:
    Steinberg::Vst::IEditController* controller{nullptr};
    Steinberg::ViewRect viewRect{0, 0, 400, 500};
    void* platformWindow{nullptr};
    Steinberg::uint32 refCount{1};
    Steinberg::TScalar scaleFactor{1.0};

    // UI state
    struct KnobState
    {
        float value{0.0f};
        bool isDragging{false};
        float dragStartY{0.0f};
        float dragStartValue{0.0f};
    };

    std::vector<KnobState> knobStates;
    void initializeKnobs();
    void drawKnob(float x, float y, float radius, float value, const char* label);
    void drawBackground();
    void handleMouseDown(float x, float y);
    void handleMouseUp(float x, float y);
    void handleMouseMove(float x, float y);
    int getKnobAtPosition(float x, float y);
};

} // namespace Pickle
