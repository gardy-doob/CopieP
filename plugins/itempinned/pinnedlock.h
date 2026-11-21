// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QString>

/**
 * Protection of pinned items (CopieP).
 *
 * Pinned items are hidden until the user authenticates (Touch ID or the
 * macOS account password). They are locked again as soon as the app loses
 * focus.
 */
namespace PinnedLock {

/// Returns true if pinned items should be hidden now.
bool isLocked();

/// Asks user to authenticate (blocking, keeps the event loop running).
/// Returns true and unlocks items on success.
bool unlock();

/// Lock items again.
void lock();

/// Platform specific authentication, implemented in pinnedlock_mac.mm on macOS.
bool authenticate(const QString &reason);

} // namespace PinnedLock
