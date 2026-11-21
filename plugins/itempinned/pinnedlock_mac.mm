// SPDX-License-Identifier: GPL-3.0-or-later

#include "pinnedlock.h"

#include <QCoreApplication>
#include <QEventLoop>
#include <QMetaObject>
#include <QPointer>

#import <Foundation/Foundation.h>
#import <LocalAuthentication/LocalAuthentication.h>

namespace PinnedLock {

bool authenticate(const QString &reason)
{
    LAContext *context = [[LAContext alloc] init];
    context.localizedFallbackTitle = @"Utiliser le mot de passe";
    context.localizedCancelTitle = @"Annuler";

    // Touch ID if available, otherwise the macOS account password.
    const LAPolicy policy = LAPolicyDeviceOwnerAuthentication;

    NSError *error = nil;
    if (![context canEvaluatePolicy:policy error:&error]) {
        [context release];
        return false;
    }

    __block bool success = false;
    QEventLoop loop;
    QPointer<QEventLoop> loopPtr(&loop);

    NSString *nsReason = reason.toNSString();
    [context evaluatePolicy:policy
            localizedReason:nsReason
                      reply:^(BOOL ok, NSError *authError) {
        Q_UNUSED(authError)
        success = ok;
        QMetaObject::invokeMethod(qApp, [loopPtr]() {
            if (loopPtr)
                loopPtr->quit();
        }, Qt::QueuedConnection);
    }];

    // Keep the UI responsive while the system dialog is shown.
    loop.exec(QEventLoop::ExcludeUserInputEvents);

    [context release];
    return success;
}

} // namespace PinnedLock
