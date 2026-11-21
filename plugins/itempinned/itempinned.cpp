// SPDX-License-Identifier: GPL-3.0-or-later

#include "itempinned.h"
#include "pinnedlock.h"

#include "common/command.h"
#include "common/contenttype.h"
#include "common/display.h"
#include "gui/fromiconid.h"

#include <QApplication>
#include <QBoxLayout>
#include <QMessageBox>
#include <QModelIndex>
#include <QPainter>
#include <QPainterPath>
#include <QGraphicsOpacityEffect>

#include <algorithm>

namespace {

const QLatin1String mimePinned("application/x-copyq-item-pinned");

bool isPinned(const QModelIndex &index)
{
    const auto dataMap = index.data(contentType::data).toMap();
    return dataMap.contains(mimePinned);
}

Command dummyPinCommand()
{
    Command c;
    c.icon = fromIconId(IconThumbtack);
    c.inMenu = true;
    return c;
}

bool containsPinnedItems(const QModelIndexList &indexList)
{
    return std::any_of( std::begin(indexList), std::end(indexList), isPinned );
}

/// Covers the content of a protected item until the user authenticates.
class LockOverlay final : public QWidget
{
public:
    explicit LockOverlay(QWidget *parent)
        : QWidget(parent)
    {
        setAttribute(Qt::WA_TransparentForMouseEvents);
        setFocusPolicy(Qt::NoFocus);
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        if ( !PinnedLock::isLocked() )
            return;

        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing);

        // Use application palette: item widgets are styled transparent.
        const QPalette pal = QApplication::palette();
        QColor bg = pal.color(QPalette::Active, QPalette::Base);
        bg.setAlpha(255);
        QColor fg = pal.color(QPalette::Active, QPalette::Text);

        const QRectF r = QRectF(rect()).adjusted(1, 1, -1, -1);
        QPainterPath path;
        path.addRoundedRect(r, 8, 8);
        p.fillPath(path, bg);
        QColor tint = fg;
        tint.setAlpha(18);
        p.fillPath(path, tint);

        QFont f = font();
        p.setFont(f);
        QColor textColor = fg;
        textColor.setAlpha(170);
        p.setPen(textColor);

        const QString text = QString::fromUtf8("\xF0\x9F\x94\x92  ")
            + ItemPinnedLoader::tr("Élément protégé — Entrée ou double-clic pour déverrouiller");
        const QFontMetrics fm(f);
        const int lineHeight = fm.height() + 8;
        QRectF textRect = r.adjusted(10, 0, -10, 0);
        if (textRect.height() > lineHeight)
            textRect.setHeight(lineHeight);
        p.drawText(textRect, Qt::AlignVCenter | Qt::AlignLeft,
                   fm.elidedText(text, Qt::ElideRight, int(textRect.width())));
    }
};

QVariantMap lockedItemData(const QVariantMap &itemData)
{
    // Keep only internal formats (pinned flag, tags...) and drop content.
    QVariantMap result;
    for (auto it = itemData.constBegin(); it != itemData.constEnd(); ++it) {
        if ( it.key().startsWith(QLatin1String("application/x-copyq-")) )
            result.insert(it.key(), it.value());
    }
    return result;
}

} // namespace

ItemPinned::ItemPinned(ItemWidget *childItem)
    : QWidget( childItem->widget()->parentWidget() )
    , ItemWidgetWrapper(childItem, this)
{
    childItem->widget()->setObjectName("item_child");
    childItem->widget()->setParent(this);

    QBoxLayout *layout;
    layout = new QHBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(childItem->widget());
    layout->setAlignment(Qt::AlignTop | Qt::AlignLeft);

    m_lockOverlay = new LockOverlay(this);
    m_lockOverlay->raise();
}

void ItemPinned::paintEvent(QPaintEvent *paintEvent)
{
    QWidget::paintEvent(paintEvent);

    QPainter painter(this);
    QColor color;
    if (m_indicatorColor.isValid()) {
        color = m_indicatorColor;
    } else {
        color = painter.pen().color();
        color.setAlpha(50);
    }
    const int border = pointsToPixels(m_indicatorWidth, this);
    painter.fillRect(width() - border, 0, border, height(), color);

    const bool locked = PinnedLock::isLocked();
    if (m_lockOverlay) {
        m_lockOverlay->setGeometry(0, 0, width() - border, height());
        if (m_lockOverlay->isVisible() != locked)
            m_lockOverlay->setVisible(locked);
        m_lockOverlay->raise();
    }

    // Make the real content fully invisible while locked (the overlay alone
    // could be translucent with some themes).
    QWidget *child = findChild<QWidget*>(QStringLiteral("item_child"), Qt::FindDirectChildrenOnly);
    if (child) {
        auto effect = qobject_cast<QGraphicsOpacityEffect*>(child->graphicsEffect());
        if (locked && !effect) {
            effect = new QGraphicsOpacityEffect(child);
            effect->setOpacity(0.0);
            child->setGraphicsEffect(effect);
        } else if (!locked && effect) {
            child->setGraphicsEffect(nullptr);
        }
    }
}

void ItemPinned::updateSize(QSize maximumSize, int idealWidth)
{
    setMinimumWidth(idealWidth);
    setMaximumWidth(maximumSize.width());
    const int border = pointsToPixels(2 * m_indicatorWidth, this);
    const int childItemWidth = idealWidth - border;
    const auto childItemMaximumSize = QSize(maximumSize.width() - border, maximumSize.height());
    ItemWidgetWrapper::updateSize(childItemMaximumSize, childItemWidth);
    adjustSize();
}

bool ItemPinnedScriptable::isPinned()
{
    const auto args = currentArguments();
    for (const auto &arg : args) {
        bool ok;
        const int row = arg.toInt(&ok);
        if (ok) {
            const auto result = call("read", {QStringLiteral("?"), row});
            if ( result.toByteArray().contains(mimePinned.data()) )
                return true;
        }
    }

    return false;
}

void ItemPinnedScriptable::pin()
{
    const auto args = currentArguments();
    if (args.isEmpty()) {
        pinData();
    } else {
        for (const auto &arg : args) {
            bool ok;
            const int row = arg.toInt(&ok);
            if (ok)
                call("change", {row, mimePinned, QString()});
        }
    }
}

void ItemPinnedScriptable::unpin()
{
    const auto args = currentArguments();
    if (args.isEmpty()) {
        unpinData();
    } else {
        for (const auto &arg : args) {
            bool ok;
            const int row = arg.toInt(&ok);
            if (ok)
                call("change", {row, mimePinned, QVariant()});
        }
    }
}

void ItemPinnedScriptable::pinData()
{
    call("setData", {mimePinned, QString()});
}

void ItemPinnedScriptable::unpinData()
{
    call("removeData", {mimePinned});
}

bool ItemPinnedScriptable::unlockProtected()
{
    return call("callPlugin", {QStringLiteral("itempinned"), QStringLiteral("unlock")}).toBool();
}

void ItemPinnedScriptable::lockProtected()
{
    call("callPlugin", {QStringLiteral("itempinned"), QStringLiteral("lock")});
}

bool ItemPinnedScriptable::isProtectedLocked()
{
    return call("callPlugin", {QStringLiteral("itempinned"), QStringLiteral("isLocked")}).toBool();
}

QString ItemPinnedScriptable::getMimePinned() const
{
    return ::mimePinned;
}

ItemPinnedSaver::ItemPinnedSaver(QAbstractItemModel *model, const ItemSaverPtr &saver)
    : ItemSaverWrapper(saver)
    , m_model(model)
{
    connect( model, &QAbstractItemModel::rowsInserted,
             this, &ItemPinnedSaver::onRowsInserted );
    connect( model, &QAbstractItemModel::rowsRemoved,
             this, &ItemPinnedSaver::onRowsRemoved );
    connect( model, &QAbstractItemModel::rowsMoved,
             this, &ItemPinnedSaver::onRowsMoved );
    connect( model, &QAbstractItemModel::dataChanged,
             this, &ItemPinnedSaver::onDataChanged );

    updateLastPinned( 0, m_model->rowCount() );
}

bool ItemPinnedSaver::canRemoveItems(const QList<QModelIndex> &indexList, QString *error)
{
    if ( !containsPinnedItems(indexList) )
        return ItemSaverWrapper::canRemoveItems(indexList, error);

    if (error) {
        *error = "Removing pinned item is not allowed (unpin item first)";
        return false;
    }

    QMessageBox::information(
                QApplication::activeWindow(),
                ItemPinnedLoader::tr("Cannot Remove Pinned Items"),
                ItemPinnedLoader::tr("Unpin items first to remove them.") );
    return false;
}

bool ItemPinnedSaver::canDropItem(const QModelIndex &index)
{
    return !isPinned(index) && ItemSaverWrapper::canDropItem(index);
}

bool ItemPinnedSaver::canMoveItems(const QList<QModelIndex> &indexList)
{
    return !containsPinnedItems(indexList)
            && ItemSaverWrapper::canMoveItems(indexList);
}

QVariantMap ItemPinnedSaver::copyItem(const QAbstractItemModel &model, const QVariantMap &itemData)
{
    const QVariantMap data = ItemSaverWrapper::copyItem(model, itemData);
    if ( itemData.contains(mimePinned) && PinnedLock::isLocked() )
        return lockedItemData(data);
    return data;
}

bool ItemPinnedSaver::isItemLocked(const QModelIndex &index)
{
    if ( isPinned(index) && PinnedLock::isLocked() )
        return true;
    return ItemSaverWrapper::isItemLocked(index);
}

bool ItemPinnedSaver::unlockItems(const QList<QModelIndex> &indexList)
{
    if ( containsPinnedItems(indexList) && !PinnedLock::unlock() )
        return false;
    return ItemSaverWrapper::unlockItems(indexList);
}

void ItemPinnedSaver::onRowsInserted(const QModelIndex &, int start, int end)
{
    if (!m_model || m_lastPinned < start) {
        updateLastPinned(start, end);
        return;
    }

    disconnect( m_model.data(), &QAbstractItemModel::rowsMoved,
                this, &ItemPinnedSaver::onRowsMoved );

    // Shift rows below inserted up.
    const int rowCount = end - start + 1;
    for (int row = end + 1; row <= m_lastPinned + rowCount; ++row) {
        const auto index = m_model->index(row, 0);
        if ( isPinned(index) )
            moveRow(row, row - rowCount);
    }

    connect( m_model.data(), &QAbstractItemModel::rowsMoved,
             this, &ItemPinnedSaver::onRowsMoved );
}

void ItemPinnedSaver::onRowsRemoved(const QModelIndex &, int start, int end)
{
    if (!m_model || m_lastPinned < start)
        return;

    disconnect( m_model.data(), &QAbstractItemModel::rowsMoved,
                this, &ItemPinnedSaver::onRowsMoved );

    // Shift rows below removed down.
    const int rowCount = end - start + 1;
    for (int row = m_lastPinned - rowCount; row >= start; --row) {
        const auto index = m_model->index(row, 0);
        if ( isPinned(index) )
            moveRow(row, row + rowCount + 1);
    }

    connect( m_model.data(), &QAbstractItemModel::rowsMoved,
             this, &ItemPinnedSaver::onRowsMoved );
}

void ItemPinnedSaver::onRowsMoved(const QModelIndex &, int start, int end, const QModelIndex &, int destinationRow)
{
    if (!m_model)
        return;

    if ( (m_lastPinned >= start || m_lastPinned >= destinationRow)
         && (end >= m_lastPinned || destinationRow >= m_lastPinned) )
    {
        if (start < destinationRow)
            updateLastPinned(start, destinationRow + end - start + 1);
        else
            updateLastPinned(destinationRow, end);
    }

    if (destinationRow != 0 || start < destinationRow)
        return;

    const int rowCount = end - start + 1;

    for (int row = destinationRow; row < destinationRow + rowCount; ++row) {
        if ( isPinned(m_model->index(row, 0)) )
            return;
    }

    disconnect( m_model.data(), &QAbstractItemModel::rowsMoved,
                this, &ItemPinnedSaver::onRowsMoved );

    // Shift rows below inserted up.
    for (int row = destinationRow + rowCount; row <= std::min(m_lastPinned, end); ++row) {
        const auto index = m_model->index(row, 0);
        if ( isPinned(index) )
            moveRow(row, row - rowCount);
    }

    connect( m_model.data(), &QAbstractItemModel::rowsMoved,
             this, &ItemPinnedSaver::onRowsMoved );
}

void ItemPinnedSaver::onDataChanged(const QModelIndex &topLeft, const QModelIndex &bottomRight)
{
    if ( bottomRight.row() < m_lastPinned )
        return;

    updateLastPinned( topLeft.row(), bottomRight.row() );
}

void ItemPinnedSaver::moveRow(int from, int to)
{
    m_model->moveRow(QModelIndex(), from, QModelIndex(), to);
}

void ItemPinnedSaver::updateLastPinned(int from, int to)
{
    for (int row = to; row >= from; --row) {
        const auto index = m_model->index(row, 0);
        if ( isPinned(index) ) {
            m_lastPinned = row;
            break;
        }
    }
}

ItemPinnedLoader::ItemPinnedLoader() = default;

ItemPinnedLoader::~ItemPinnedLoader() = default;

QStringList ItemPinnedLoader::formatsToSave() const
{
    return QStringList() << mimePinned;
}

ItemWidget *ItemPinnedLoader::transform(ItemWidget *itemWidget, const QVariantMap &data)
{
    return data.contains(mimePinned) ? new ItemPinned(itemWidget) : nullptr;
}

ItemSaverPtr ItemPinnedLoader::transformSaver(const ItemSaverPtr &saver, QAbstractItemModel *model)
{
    return std::make_shared<ItemPinnedSaver>(model, saver);
}

QVariant ItemPinnedLoader::scriptCallback(const QVariantList &arguments)
{
    const QString action = arguments.value(0).toString();
    if (action == QLatin1String("unlock"))
        return PinnedLock::unlock();
    if (action == QLatin1String("lock")) {
        PinnedLock::lock();
        return true;
    }
    if (action == QLatin1String("isLocked"))
        return PinnedLock::isLocked();
    return QVariant();
}

ItemScriptable *ItemPinnedLoader::scriptableObject()
{
    return new ItemPinnedScriptable();
}

QVector<Command> ItemPinnedLoader::commands() const
{
    QVector<Command> commands;

    Command c;

    c = dummyPinCommand();
    c.internalId = QStringLiteral("copyq_pinned_pin");
    c.name = tr("Pin");
    c.input = "!OUTPUT";
    c.output = mimePinned;
    c.cmd = "copyq: plugins.itempinned.pin()";
    commands.append(c);

    c = dummyPinCommand();
    c.internalId = QStringLiteral("copyq_pinned_unpin");
    c.name = tr("Unpin");
    c.input = mimePinned;
    c.cmd = "copyq: plugins.itempinned.unpin()";
    commands.append(c);

    c = Command();
    c.icon = fromIconId(IconLock);
    c.inMenu = true;
    c.internalId = QStringLiteral("copiep_pinned_unlock");
    c.name = QString::fromUtf8("Déverrouiller les éléments protégés");
    c.cmd = "copyq: plugins.itempinned.unlockProtected()";
    c.shortcuts = QStringList{QStringLiteral("ctrl+u")};
    commands.append(c);

    c = Command();
    c.icon = fromIconId(IconLock);
    c.inMenu = true;
    c.internalId = QStringLiteral("copiep_pinned_lock");
    c.name = QString::fromUtf8("Verrouiller les éléments protégés");
    c.cmd = "copyq: plugins.itempinned.lockProtected()";
    commands.append(c);

    return commands;
}
