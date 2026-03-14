#include "pickle_view.h"
#include "pickle_processor.h"
#include <cmath>
#include <cstring>

namespace Pickle {

PickleView::PickleView(Steinberg::Vst::IEditController* ctrl)
    : controller(ctrl)
{
    controller->addRef();
    initializeKnobs();
}

void PickleView::initializeKnobs()
{
    knobStates.resize(8); // 8 parameters total
    for (auto& state : knobStates)
    {
        state.value = 0.0f;
        state.isDragging = false;
    }
}

Steinberg::tresult PLUGIN_API PickleView::attached(void* parent, Steinberg::FIDString type)
{
    if (!parent)
        return Steinberg::kInvalidArgument;

    platformWindow = parent;
    
    // In a real implementation, you would:
    // 1. Create a native window/view based on the platform
    // 2. Set up OpenGL/Direct2D/other graphics context
    // 3. Initialize UI rendering system
    
    // For now, we'll simulate the UI setup
    drawBackground();
    
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::removed()
{
    platformWindow = nullptr;
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::onSize(Steinberg::ViewRect* newSize)
{
    if (newSize)
    {
        viewRect = *newSize;
        // Redraw UI with new size
        drawBackground();
    }
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::getSize(Steinberg::ViewRect* size)
{
    if (size)
    {
        *size = viewRect;
    }
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::canResize()
{
    return Steinberg::kResultTrue;
}

Steinberg::tresult PLUGIN_API PickleView::checkSizeConstraint(Steinberg::ViewRect* rect)
{
    if (rect)
    {
        // Minimum size: 300x400, Maximum size: 800x1000
        if (rect->getWidth() < 300) rect->right = rect->left + 300;
        if (rect->getHeight() < 400) rect->bottom = rect->top + 400;
        if (rect->getWidth() > 800) rect->right = rect->left + 800;
        if (rect->getHeight() > 1000) rect->bottom = rect->top + 1000;
    }
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::setContentScaleFactor(Steinberg::TScalar factor)
{
    scaleFactor = factor;
    // Adjust UI elements for scaling
    return Steinberg::kResultOk;
}

Steinberg::tresult PLUGIN_API PickleView::queryInterface(const Steinberg::TUID iid, void** obj)
{
    // Query interface for supported interfaces
    if (Steinberg::FUnknownPrivate::iidEqual(iid, Steinberg::Vst::IPlugView::iid))
    {
        *obj = static_cast<Steinberg::Vst::IPlugView*>(this);
        addRef();
        return Steinberg::kResultOk;
    }
    *obj = nullptr;
    return Steinberg::kNoInterface;
}

Steinberg::uint32 PLUGIN_API PickleView::addRef()
{
    return ++refCount;
}

Steinberg::uint32 PLUGIN_API PickleView::release()
{
    if (--refCount == 0)
    {
        delete this;
        return 0;
    }
    return refCount;
}

void PickleView::drawBackground()
{
    // In a real implementation, this would render the blurred glass background
    // using platform-specific graphics APIs (Direct2D, Core Graphics, etc.)
    
    // Simulated drawing operations:
    // 1. Draw blurred glass background with gradient
    // 2. Draw border with transparency
    // 3. Draw title and subtitle
    // 4. Draw all knobs with current values
    // 5. Draw preset buttons
    
    // Draw main processor knob (larger, centered)
    float centerX = viewRect.getWidth() / 2.0f;
    float centerY = viewRect.getHeight() / 2.0f + 50.0f;
    drawKnob(centerX, centerY, 45.0f, knobStates[5].value, "PROCESSOR");
    
    // Draw smaller knobs in grid layout
    float knobX[] = {100.0f, 200.0f, 300.0f, 100.0f, 200.0f, 300.0f};
    float knobY[] = {150.0f, 150.0f, 150.0f, 250.0f, 250.0f, 250.0f};
    const char* labels[] = {"BRINE", "SPICE", "FERMENT", "JUICE", "CRUNCH", "BITCRUSH"};
    int paramIndices[] = {0, 2, 3, 4, 6, 7};
    
    for (int i = 0; i < 6; i++)
    {
        float radius = (i == 4) ? 35.0f : 30.0f; // Crunch knob slightly larger
        drawKnob(knobX[i], knobY[i], radius, knobStates[paramIndices[i]].value, labels[i]);
    }
}

void PickleView::drawKnob(float x, float y, float radius, float value, const char* label)
{
    // In a real implementation, this would:
    // 1. Draw blurred glass circle with backdrop filter
    // 2. Draw gradient border
    // 3. Draw indicator based on value (rotation)
    // 4. Draw label text
    // 5. Draw value text
    
    // Simulated drawing:
    // - Create circular path
    // - Fill with rgba(255,255,255,0.15) + blur
    // - Stroke with rgba(144,238,144,0.5)
    // - Draw indicator line rotated by (value * 270 - 135) degrees
    // - Draw text label below
}

void PickleView::handleMouseDown(float x, float y)
{
    int knobIndex = getKnobAtPosition(x, y);
    if (knobIndex >= 0)
    {
        knobStates[knobIndex].isDragging = true;
        knobStates[knobIndex].dragStartY = y;
        knobStates[knobIndex].dragStartValue = knobStates[knobIndex].value;
        
        // Update controller parameter
        if (controller)
        {
            Steinberg::Vst::ParameterID paramId = static_cast<Steinberg::Vst::ParameterID>(knobIndex);
            controller->setParamNormalized(paramId, knobStates[knobIndex].value);
            controller->beginEdit(paramId);
        }
    }
}

void PickleView::handleMouseUp(float x, float y)
{
    for (auto& state : knobStates)
    {
        if (state.isDragging)
        {
            state.isDragging = false;
            if (controller)
            {
                // End edit for all parameters
                for (int i = 0; i < 8; i++)
                {
                    controller->endEdit(static_cast<Steinberg::Vst::ParameterID>(i));
                }
            }
        }
    }
}

void PickleView::handleMouseMove(float x, float y)
{
    for (int i = 0; i < knobStates.size(); i++)
    {
        if (knobStates[i].isDragging)
        {
            // Calculate new value based on mouse movement
            float deltaY = knobStates[i].dragStartY - y;
            float newValue = knobStates[i].dragStartValue + (deltaY / 100.0f);
            newValue = std::max(0.0f, std::min(1.0f, newValue));
            
            knobStates[i].value = newValue;
            
            // Update controller parameter
            if (controller)
            {
                Steinberg::Vst::ParameterID paramId = static_cast<Steinberg::Vst::ParameterID>(i);
                controller->setParamNormalized(paramId, newValue);
                controller->performEdit(paramId, newValue);
            }
            
            // Redraw UI
            drawBackground();
        }
    }
}

int PickleView::getKnobAtPosition(float x, float y)
{
    // Define knob positions and sizes
    struct KnobInfo {
        float x, y, radius;
        int index;
    };
    
    KnobInfo knobs[] = {
        {100.0f, 150.0f, 30.0f, 0}, // BRINE
        {200.0f, 150.0f, 30.0f, 2}, // SPICE  
        {300.0f, 150.0f, 30.0f, 3}, // FERMENT
        {100.0f, 250.0f, 30.0f, 4}, // JUICE
        {200.0f, 250.0f, 35.0f, 6}, // CRUNCH
        {300.0f, 250.0f, 25.0f, 7}, // BITCRUSH
        {200.0f, 350.0f, 45.0f, 5}  // PROCESSOR
    };
    
    for (const auto& knob : knobs)
    {
        float dx = x - knob.x;
        float dy = y - knob.y;
        float distance = std::sqrt(dx * dx + dy * dy);
        
        if (distance <= knob.radius)
        {
            return knob.index;
        }
    }
    
    return -1; // No knob at this position
}

} // namespace Pickle
