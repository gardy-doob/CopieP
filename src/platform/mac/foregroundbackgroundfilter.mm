// SPDX-License-Identifier: GPL-3.0-or-later

#include "foregroundbackgroundfilter.h"

#include "common/log.h"

#include <QApplication>
#include <QEvent>
#include <QGuiApplication>
#include <QWidget>
#include <QWindow>

#include <Cocoa/Cocoa.h>
#include <Carbon/Carbon.h>

namespace {

bool isNormalApp()
{
    return [NSApp activationPolicy] == NSApplicationActivationPolicyRegular;
}

void becomeNormalApp()
{
    if (!isNormalApp()) {
        COPYQ_LOG("Become normal app");
        if (![NSApp setActivationPolicy:NSApplicationActivationPolicyRegular])
            COPYQ_LOG("Unable to become a normal app");
    }
}

void becomeAccessoryApp()
{
    if ([NSApp activationPolicy] != NSApplicationActivationPolicyAccessory) {
        COPYQ_LOG("Become accessory app");
        if (![NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory])
            COPYQ_LOG("Unable to become a accessory app");
    }
}

void becomeBackgroundApp()
{
    if ([NSApp activationPolicy] != NSApplicationActivationPolicyProhibited) {
        COPYQ_LOG("Become background app");
        if (![NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited])
            COPYQ_LOG("Unable to become a background app");
    }
}

// CopieP: the main clipboard window behaves like a panel (no Dock icon,
// shown over full screen apps), like the Windows 11 clipboard.
bool isMainWindow(QWindow *window)
{
    for (QWidget *widget : QApplication::topLevelWidgets()) {
        if (widget->windowHandle() == window)
            return widget->inherits("MainWindow");
    }
    return false;
}

bool isNormalAppWindow(QWindow *window)
{
    const auto type = window->type();
    return (type == Qt::Dialog || type == Qt::Window)
            && !window->flags().testFlag(Qt::FramelessWindowHint)
            && !isMainWindow(window);
}

} // namespace

void ForegroundBackgroundFilter::installFilter(QObject *parent)
{
    auto filter = new ForegroundBackgroundFilter(parent);
    parent->installEventFilter(filter);
}

ForegroundBackgroundFilter::ForegroundBackgroundFilter(QObject *parent)
    : QObject(parent)
{
}

bool ForegroundBackgroundFilter::eventFilter(QObject *obj, QEvent *ev)
{
    auto window = qobject_cast<QWindow*>(obj);
    if ( !window || !window->isTopLevel() )
        return false;

    const auto eventType = ev->type();
    if (eventType == QEvent::Show) {
        // WORKAROUND: Transform menu to frameless dialog to fix keyboard focus.
        if (window->type() == Qt::Popup && QGuiApplication::focusWindow() == nullptr) {
            COPYQ_LOG("Fixing focus for popup");
            window->setFlags(window->flags() | Qt::Dialog | Qt::FramelessWindowHint);
        }

        // Allow windows to pop up on current space.
        const auto wid = window->winId();
        auto nsView = reinterpret_cast<NSView*>(wid);
        auto nsWindow = [nsView window];
        if (nsWindow) {
            if ( isMainWindow(window) ) {
                // Visible on every space, including over full screen apps.
                [nsWindow setCollectionBehavior:
                    NSWindowCollectionBehaviorCanJoinAllSpaces
                    | NSWindowCollectionBehaviorFullScreenAuxiliary];
                [nsWindow setLevel: NSFloatingWindowLevel];
                [nsWindow setHidesOnDeactivate: NO];
            } else {
                [nsWindow setCollectionBehavior:
                    NSWindowCollectionBehaviorMoveToActiveSpace
                    | NSWindowCollectionBehaviorFullScreenAuxiliary];
            }
        }

        if ( isNormalAppWindow(window) )
            becomeNormalApp();
        else if (!isNormalApp())
            becomeAccessoryApp();
    } else if (eventType == QEvent::Hide) {
        // Don't background if there are any other visible windows..
        auto windows = QGuiApplication::topLevelWindows();
        windows.removeOne(window);

        const bool hasFocusableWindow = std::any_of(
                    std::begin(windows), std::end(windows),
                    [](QWindow *aWindow) {
                        return aWindow->isVisible() && isNormalAppWindow(aWindow);
                    });

        if (hasFocusableWindow) {
            becomeNormalApp();
        } else {
            const bool hasAccessoryWindow = std::any_of(
                        std::begin(windows), std::end(windows),
                        [](QWindow *aWindow) {
                            return aWindow->isVisible();
                        });

            if (hasAccessoryWindow)
                becomeAccessoryApp();
            else
                becomeBackgroundApp(); // This also focuses the previous app.
        }
    }

    return false;
}
