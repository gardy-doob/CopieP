// SPDX-License-Identifier: GPL-3.0-or-later

#include "pinnedlock.h"

#include <QApplication>
#include <QGuiApplication>
#include <QMessageBox>
#include <QObject>

namespace {

bool g_unlocked = false;
bool g_authenticating = false;

bool lockDisabled()
{
    // Used by automated tests.
    return !qEnvironmentVariableIsEmpty("COPIEP_DISABLE_PINNED_LOCK");
}

void ensureAutoLock()
{
    static bool connected = false;
    if (connected || !qApp)
        return;
    connected = true;

    // Lock again whenever the app loses focus (window hidden, item pasted
    // into another app, user switched apps...).
    QObject::connect(
        qApp, &QGuiApplication::applicationStateChanged, qApp,
        [](Qt::ApplicationState state) {
            if (state != Qt::ApplicationActive && !g_authenticating)
                PinnedLock::lock();
        });
}

} // namespace

namespace PinnedLock {

bool isLocked()
{
    ensureAutoLock();
    return !g_unlocked && !lockDisabled();
}

bool unlock()
{
    if ( !isLocked() )
        return true;

    if (g_authenticating)
        return false;

    g_authenticating = true;
    const bool ok = authenticate(
        QObject::tr("afficher et coller les éléments protégés"));
    g_authenticating = false;

    if (ok) {
        g_unlocked = true;
        for (QWidget *w : QApplication::topLevelWidgets()) {
            if (w->isVisible())
                w->update();
        }
    }
    return ok;
}

void lock()
{
    if (!g_unlocked)
        return;
    g_unlocked = false;
    for (QWidget *w : QApplication::topLevelWidgets()) {
        if (w->isVisible())
            w->update();
    }
}

#ifndef Q_OS_MAC
bool authenticate(const QString &)
{
    // No biometric API on this platform: simple confirmation.
    return QMessageBox::question(
        QApplication::activeWindow(),
        QObject::tr("Éléments protégés"),
        QObject::tr("Afficher les éléments protégés ?")) == QMessageBox::Yes;
}
#endif

} // namespace PinnedLock
