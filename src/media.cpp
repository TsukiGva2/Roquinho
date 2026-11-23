#include "media.h"

#include <QCryptographicHash>
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusVariant>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QStandardPaths>
#include <QUrl>

#include <algorithm>
#include <cmath>
#include <complex>

namespace {

const QString kPrefix = QStringLiteral("org.mpris.MediaPlayer2.");
const QString kPath = QStringLiteral("/org/mpris/MediaPlayer2");
const QString kRoot = QStringLiteral("org.mpris.MediaPlayer2");
const QString kPlayer = QStringLiteral("org.mpris.MediaPlayer2.Player");
const QString kProps = QStringLiteral("org.freedesktop.DBus.Properties");

constexpr int kRate = 44100;
constexpr int kFft = 1024;
constexpr int kRing = 2048;
constexpr int kBands = 24;

qint64 nowMs() { return QDateTime::currentMSecsSinceEpoch(); }

QVariant unwrap(const QVariant &v)
{
    if (v.metaType() == QMetaType::fromType<QDBusVariant>())
        return v.value<QDBusVariant>().variant();
    return v;
}

QVariantMap toMap(const QVariant &raw)
{
    const QVariant v = unwrap(raw);
    if (v.metaType() == QMetaType::fromType<QDBusArgument>())
        return qdbus_cast<QVariantMap>(v.value<QDBusArgument>());
    return v.toMap();
}

QStringList toStringList(const QVariant &raw)
{
    const QVariant v = unwrap(raw);
    if (v.metaType() == QMetaType::fromType<QDBusArgument>())
        return qdbus_cast<QStringList>(v.value<QDBusArgument>());
    if (v.metaType() == QMetaType::fromType<QString>())
        return {v.toString()};
    return v.toStringList();
}

QString objectPath(const QVariant &raw)
{
    const QVariant v = unwrap(raw);
    if (v.metaType() == QMetaType::fromType<QDBusObjectPath>())
        return v.value<QDBusObjectPath>().path();
    return v.toString();
}

bool containsAny(const QString &hay, std::initializer_list<const char *> needles)
{
    for (const char *n : needles)
        if (hay.contains(QLatin1String(n)))
            return true;
    return false;
}

// Pai de um processo (para casar o stream de áudio com o player).
uint parentPid(uint pid)
{
    QFile f(QStringLiteral("/proc/%1/stat").arg(pid));
    if (!f.open(QIODevice::ReadOnly))
        return 0;
    const QByteArray s = f.readAll();
    const int close = s.lastIndexOf(')');
    if (close < 0)
        return 0;
    const QList<QByteArray> parts = s.mid(close + 2).split(' ');
    return parts.size() > 1 ? parts.at(1).toUInt() : 0;
}

bool isAncestor(uint ancestor, uint pid)
{
    for (int i = 0; i < 12 && pid > 1; ++i) {
        if (pid == ancestor)
            return true;
        pid = parentPid(pid);
    }
    return false;
}

void fft(std::vector<std::complex<float>> &a)
{
    const size_t n = a.size();
    for (size_t i = 1, j = 0; i < n; ++i) {
        size_t bit = n >> 1;
        for (; j & bit; bit >>= 1)
            j ^= bit;
        j ^= bit;
        if (i < j)
            std::swap(a[i], a[j]);
    }
    for (size_t len = 2; len <= n; len <<= 1) {
        const float ang = -2.0f * float(M_PI) / float(len);
        const std::complex<float> wl(std::cos(ang), std::sin(ang));
        for (size_t i = 0; i < n; i += len) {
            std::complex<float> w(1);
            for (size_t j = 0; j < len / 2; ++j) {
                const std::complex<float> u = a[i + j], v = a[i + j + len / 2] * w;
                a[i + j] = u + v;
                a[i + j + len / 2] = u - v;
                w *= wl;
            }
        }
    }
}

// Cor marcante da capa (e uma segunda, para degradês), já clareadas para o fundo preto.
void pickColors(const QImage &src, QColor &main, QColor &second)
{
    main = second = QColor();
    if (src.isNull())
        return;
    const QImage img = src.convertToFormat(QImage::Format_RGB32).scaled(36, 36, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    constexpr int kBuckets = 18;
    double weight[kBuckets] = {};
    double r[kBuckets] = {}, g[kBuckets] = {}, b[kBuckets] = {};
    double total = 0;
    for (int y = 0; y < img.height(); ++y) {
        for (int x = 0; x < img.width(); ++x) {
            const QColor c = QColor::fromRgb(img.pixel(x, y));
            const double s = c.hsvSaturationF(), v = c.valueF();
            if (v < 0.18 || s < 0.22)
                continue;
            const double w = s * s * v;
            const int h = std::clamp(int(c.hsvHueF() * kBuckets), 0, kBuckets - 1);
            weight[h] += w;
            r[h] += c.redF() * w;
            g[h] += c.greenF() * w;
            b[h] += c.blueF() * w;
            total += w;
        }
    }
    auto finish = [](double rr, double gg, double bb) {
        QColor c = QColor::fromRgbF(float(rr), float(gg), float(bb));
        float h, s, l, a;
        c.getHslF(&h, &s, &l, &a);
        s = std::max(s, 0.55f);
        l = std::clamp(l, 0.58f, 0.72f);
        return QColor::fromHslF(h, s, l);
    };
    if (total < 6.0) { // capa praticamente sem cor: tons neutros elegantes
        main = QColor(0xe5, 0xe5, 0xea);
        second = QColor(0x8e, 0x8e, 0x93);
        return;
    }
    int best = 0;
    for (int i = 1; i < kBuckets; ++i)
        if (weight[i] > weight[best])
            best = i;
    // junta os vizinhos para a média não ficar "picotada"
    auto merged = [&](int c, double &rr, double &gg, double &bb) {
        double w = 0;
        rr = gg = bb = 0;
        for (int d = -1; d <= 1; ++d) {
            const int i = (c + d + kBuckets) % kBuckets;
            w += weight[i];
            rr += r[i];
            gg += g[i];
            bb += b[i];
        }
        rr /= w;
        gg /= w;
        bb /= w;
    };
    double rr, gg, bb;
    merged(best, rr, gg, bb);
    main = finish(rr, gg, bb);

    int alt = -1;
    for (int i = 0; i < kBuckets; ++i) {
        const int dist = std::min(std::abs(i - best), kBuckets - std::abs(i - best));
        if (dist >= 3 && weight[i] > total * 0.06 && (alt < 0 || weight[i] > weight[alt]))
            alt = i;
    }
    if (alt >= 0) {
        merged(alt, rr, gg, bb);
        second = finish(rr, gg, bb);
    } else {
        float h, s, l, a;
        main.getHslF(&h, &s, &l, &a);
        second = QColor::fromHslF(std::fmod(h + 0.08f, 1.0f), s, std::max(0.0f, l - 0.18f));
    }
}

} // namespace

Media::Media(QObject *parent)
    : QObject(parent)
    , m_net(new QNetworkAccessManager(this))
    , m_ring(kRing, 0.0f)
    , m_bands(kBands, 0.0f)
    , m_peak(kBands, 0.0f)
{
    for (int i = 0; i < kBands; ++i)
        m_levelsOut.append(0.0);
    m_sinceBeat.start();

    QDBusConnection bus = QDBusConnection::sessionBus();
    bus.connect(QStringLiteral("org.freedesktop.DBus"), QStringLiteral("/org/freedesktop/DBus"), QStringLiteral("org.freedesktop.DBus"),
                QStringLiteral("NameOwnerChanged"), this, SLOT(onNameOwnerChanged(QString,QString,QString)));
    bus.connect(QString(), kPath, kProps, QStringLiteral("PropertiesChanged"), this,
                SLOT(onPropertiesChanged(QString,QVariantMap,QStringList)));
    bus.connect(QString(), kPath, kPlayer, QStringLiteral("Seeked"), this, SLOT(onSeeked(qlonglong)));

    const QStringList names = bus.interface()->registeredServiceNames().value();
    for (const QString &name : names)
        if (name.startsWith(kPrefix))
            addPlayer(name, bus.interface()->serviceOwner(name).value());

    m_positionPoll.setInterval(1000);
    connect(&m_positionPoll, &QTimer::timeout, this, &Media::pollPosition);

    m_frame.setInterval(33);
    connect(&m_frame, &QTimer::timeout, this, &Media::analyze);
    // O stream do player pode surgir depois do play; reavalia o alvo da captura de vez em quando.
    m_retarget.setInterval(4000);
    connect(&m_retarget, &QTimer::timeout, this, [this] {
        if (m_capture.state() != QProcess::NotRunning && captureTarget() != m_captureArgs) {
            stopCapture();
            startCapture();
        }
    });

    connect(&m_capture, &QProcess::readyReadStandardOutput, this, &Media::readAudio);
    connect(&m_capture, &QProcess::finished, this, [this] {
        // parec caiu (stream do player recriado, troca de saída de áudio...): tenta de novo.
        if (m_captureClock.elapsed() < 1500 && m_captureArgs.startsWith(QLatin1String("--monitor-stream")))
            m_streamBroken = true;
        QTimer::singleShot(m_captureClock.elapsed() < 1500 ? 2500 : 700, this, &Media::updateAnalyzer);
    });
}

Media::~Media()
{
    stopCapture();
}

void Media::setEnabled(bool on)
{
    if (on == m_enabled)
        return;
    m_enabled = on;
    emit enabledChanged();
    emit trackChanged();
    emit playingChanged();
    emit artChanged();
    updateAnalyzer();
}

void Media::setVisualize(bool on)
{
    if (on == m_visualize)
        return;
    m_visualize = on;
    emit visualizeChanged();
    updateAnalyzer();
}

// ---------------------------------------------------------------- players

void Media::addPlayer(const QString &service, const QString &owner)
{
    Player p;
    p.service = service;
    p.owner = owner;
    p.pid = QDBusConnection::sessionBus().interface()->servicePid(service).value();
    m_players.insert(service, p);

    for (const QString &iface : {kRoot, kPlayer}) {
        QDBusMessage msg = QDBusMessage::createMethodCall(service, kPath, kProps, QStringLiteral("GetAll"));
        msg << iface;
        auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(msg, 2000), this);
        connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, service](QDBusPendingCallWatcher *w) {
            w->deleteLater();
            QDBusPendingReply<QVariantMap> reply = *w;
            auto it = m_players.find(service);
            if (reply.isError() || it == m_players.end())
                return;
            applyPlayerProps(*it, reply.value());
            choose();
        });
    }
}

void Media::removePlayer(const QString &service)
{
    if (m_players.remove(service))
        choose();
}

void Media::onNameOwnerChanged(const QString &name, const QString &, const QString &newOwner)
{
    if (!name.startsWith(kPrefix))
        return;
    m_players.remove(name);
    if (newOwner.isEmpty())
        choose();
    else
        addPlayer(name, newOwner);
}

void Media::applyPlayerProps(Player &p, const QVariantMap &props)
{
    for (auto it = props.begin(); it != props.end(); ++it) {
        const QString &key = it.key();
        const QVariant v = unwrap(it.value());
        if (key == QLatin1String("Identity")) {
            p.identity = v.toString();
        } else if (key == QLatin1String("DesktopEntry")) {
            p.desktopEntry = v.toString();
        } else if (key == QLatin1String("CanRaise")) {
            p.canRaise = v.toBool();
        } else if (key == QLatin1String("PlaybackStatus")) {
            const QString status = v.toString();
            if (status == QLatin1String("Playing") || p.status == QLatin1String("Playing"))
                p.lastActive = nowMs();
            p.status = status;
        } else if (key == QLatin1String("Metadata")) {
            p.meta = toMap(v);
            p.lastActive = std::max(p.lastActive, p.status == QLatin1String("Playing") ? nowMs() : p.lastActive);
        } else if (key == QLatin1String("CanGoNext")) {
            p.canNext = v.toBool();
        } else if (key == QLatin1String("CanGoPrevious")) {
            p.canPrevious = v.toBool();
        } else if (key == QLatin1String("CanSeek")) {
            p.canSeek = v.toBool();
        } else if (key == QLatin1String("Position") && p.service == m_current) {
            m_position = v.toLongLong() / 1000.0;
            m_positionStamp = double(nowMs());
            emit positionChanged();
        }
    }
}

void Media::onPropertiesChanged(const QString &iface, const QVariantMap &changed, const QStringList &)
{
    if (iface != kPlayer && iface != kRoot)
        return;
    const QString sender = message().service();
    for (auto it = m_players.begin(); it != m_players.end(); ++it) {
        if (it->owner != sender)
            continue;
        const QString oldStatus = it->status;
        applyPlayerProps(*it, changed);
        if (it.key() == m_current && (changed.contains(QStringLiteral("PlaybackStatus")) || changed.contains(QStringLiteral("Metadata"))))
            pollPosition();
        choose();
        if (it.key() == m_current)
            emit capsChanged();
        Q_UNUSED(oldStatus)
        return;
    }
}

void Media::onSeeked(qlonglong us)
{
    const Player *p = cur();
    if (!p || p->owner != message().service())
        return;
    m_position = us / 1000.0;
    m_positionStamp = double(nowMs());
    emit positionChanged();
}

bool Media::isMusic(const Player &p) const
{
    const QString title = p.meta.value(QStringLiteral("xesam:title")).toString().trimmed();
    if (title.isEmpty())
        return false;
    const QString id = (p.service.mid(kPrefix.size()) + QLatin1Char(' ') + p.identity + QLatin1Char(' ') + p.desktopEntry).toLower();
    const QString url = p.meta.value(QStringLiteral("xesam:url")).toString().toLower();

    // Apps de música: sempre valem.
    if (containsAny(id, {"spotify", "spotube", "rhythmbox", "elisa", "strawberry", "clementine", "audacious", "amarok",
                         "lollypop", "cantata", "quodlibet", "deadbeef", "cmus", "mpd", "tidal", "youtube-music",
                         "youtubemusic", "cider", "deezer", "harmonoid", "amberol", "gapless", "g4music", "tauon",
                         "sayonara", "juk", "musique", "museeks", "nuclear", "feishin", "sonixd", "supersonic",
                         "ncspot", "psst", "kdeconnect", "shortwave", "apple music", "qmmp", "goodvibes"}))
        return true;

    // Navegadores: só abas de sites de música (o WhatsApp Web, vídeos etc. ficam de fora).
    if (containsAny(id, {"firefox", "zen", "chrom", "brave", "vivaldi", "edge", "opera", "librewolf", "floorp",
                         "waterfox", "plasma-browser-integration", "browser", "epiphany", "falkon"})) {
        return containsAny(url, {"open.spotify.com", "music.youtube.com", "soundcloud.com", "deezer.com", "tidal.com",
                                 "music.apple.com", "music.amazon", "bandcamp.com", "pandora.com", "radio", "last.fm",
                                 "napster.com", "qobuz.com", "audiomack.com", "jiosaavn.com", "mixcloud.com"});
    }

    // Players de vídeo: só arquivos de áudio.
    if (containsAny(id, {"vlc", "mpv", "celluloid", "haruna", "smplayer", "totem", "dragon", "kodi", "showtime"})) {
        return containsAny(url, {".mp3", ".flac", ".ogg", ".opus", ".m4a", ".wav", ".aac", ".wma", ".alac", ".aiff"});
    }

    // Desconhecido: aceita se parecer faixa de música de verdade.
    return !toStringList(p.meta.value(QStringLiteral("xesam:artist"))).join(QString()).trimmed().isEmpty()
        && p.meta.value(QStringLiteral("mpris:length")).toLongLong() > 30 * 1000000LL;
}

void Media::choose()
{
    QString best;
    int bestScore = -1;
    qint64 bestActive = -1;
    for (auto it = m_players.cbegin(); it != m_players.cend(); ++it) {
        if (!isMusic(*it))
            continue;
        int score = 0;
        if (it->status == QLatin1String("Playing"))
            score += 4;
        if (it.key() == m_current)
            score += 1; // evita ficar pulando entre dois players pausados
        if (it->service.contains(QLatin1String("spotify"), Qt::CaseInsensitive))
            score += 2;
        if (score > bestScore || (score == bestScore && it->lastActive > bestActive)) {
            best = it.key();
            bestScore = score;
            bestActive = it->lastActive;
        }
    }

    const bool wasPlaying = playing();
    const QString oldArt = m_artUrl;
    const bool switched = best != m_current;
    m_current = best;

    const QString key = active() ? m_current + title() + artist() : QString();
    const bool trackSwitched = key != m_lastTrackKey;
    const bool hadTrack = !m_lastTrackKey.isEmpty();
    m_lastTrackKey = key;

    if (switched || trackSwitched) {
        emit trackChanged();
        emit capsChanged();
        pollPosition();
        loadArt();
        if (trackSwitched && hadTrack && !key.isEmpty() && playing())
            emit newTrack();
    }
    if (wasPlaying != playing() || switched) {
        emit playingChanged();
        if (playing() && !switched && !trackSwitched)
            pollPosition();
    }
    m_positionPoll.setInterval(playing() ? 1000 : 4000);
    if (active())
        m_positionPoll.start();
    else
        m_positionPoll.stop();
    Q_UNUSED(oldArt)
    updateAnalyzer();
}

const Media::Player *Media::cur() const
{
    if (!m_enabled || m_current.isEmpty())
        return nullptr;
    auto it = m_players.constFind(m_current);
    return it == m_players.cend() ? nullptr : &*it;
}

bool Media::playing() const
{
    const Player *p = cur();
    return p && p->status == QLatin1String("Playing");
}

QString Media::title() const
{
    const Player *p = cur();
    return p ? p->meta.value(QStringLiteral("xesam:title")).toString() : QString();
}

QString Media::artist() const
{
    const Player *p = cur();
    return p ? toStringList(p->meta.value(QStringLiteral("xesam:artist"))).join(QStringLiteral(", ")) : QString();
}

QString Media::album() const
{
    const Player *p = cur();
    return p ? p->meta.value(QStringLiteral("xesam:album")).toString() : QString();
}

QString Media::player() const
{
    const Player *p = cur();
    if (!p)
        return QString();
    if (!p->identity.isEmpty())
        return p->identity;
    QString s = p->service.mid(kPrefix.size()).section(QLatin1Char('.'), 0, 0);
    if (!s.isEmpty())
        s[0] = s[0].toUpper();
    return s;
}

double Media::length() const
{
    const Player *p = cur();
    return p ? p->meta.value(QStringLiteral("mpris:length")).toLongLong() / 1000.0 : 0.0;
}

bool Media::canNext() const { const Player *p = cur(); return p && p->canNext; }
bool Media::canPrevious() const { const Player *p = cur(); return p && p->canPrevious; }
bool Media::canSeek() const { const Player *p = cur(); return p && p->canSeek && length() > 0; }

void Media::call(const QString &method, const QVariantList &args)
{
    const Player *p = cur();
    if (!p)
        return;
    QDBusMessage msg = QDBusMessage::createMethodCall(p->service, kPath, method == QLatin1String("Raise") ? kRoot : kPlayer, method);
    msg.setArguments(args);
    QDBusConnection::sessionBus().asyncCall(msg, 2000);
}

void Media::playPause()
{
    // resposta imediata na interface; o player confirma pelo PropertiesChanged
    auto it = m_players.find(m_current);
    if (it != m_players.end()) {
        if (it->status == QLatin1String("Playing")) {
            m_position += double(nowMs()) - m_positionStamp;
            m_positionStamp = double(nowMs());
        } else {
            m_positionStamp = double(nowMs());
        }
        it->status = it->status == QLatin1String("Playing") ? QStringLiteral("Paused") : QStringLiteral("Playing");
        emit positionChanged();
        emit playingChanged();
        updateAnalyzer();
    }
    call(QStringLiteral("PlayPause"));
}

void Media::next() { call(QStringLiteral("Next")); }
void Media::previous() { call(QStringLiteral("Previous")); }

void Media::seek(double ms)
{
    const Player *p = cur();
    if (!p || !canSeek())
        return;
    const QString track = objectPath(p->meta.value(QStringLiteral("mpris:trackid")));
    if (track.isEmpty())
        return;
    call(QStringLiteral("SetPosition"), {QVariant::fromValue(QDBusObjectPath(track)), qlonglong(ms * 1000.0)});
    m_position = ms;
    m_positionStamp = double(nowMs());
    emit positionChanged();
}

void Media::raise()
{
    const Player *p = cur();
    if (!p)
        return;
    // O Spotify no Linux ignora o Raise do MPRIS; abrir o .desktop de novo traz a janela existente.
    if (!p->desktopEntry.isEmpty() && (!p->canRaise || p->service.contains(QLatin1String("spotify"))))
        QProcess::startDetached(QStringLiteral("gtk-launch"), {p->desktopEntry});
    else
        call(QStringLiteral("Raise"));
}

void Media::pollPosition()
{
    const Player *p = cur();
    if (!p)
        return;
    QDBusMessage msg = QDBusMessage::createMethodCall(p->service, kPath, kProps, QStringLiteral("Get"));
    msg << kPlayer << QStringLiteral("Position");
    const QString service = p->service;
    auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(msg, 1500), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, service](QDBusPendingCallWatcher *w) {
        w->deleteLater();
        QDBusPendingReply<QDBusVariant> reply = *w;
        if (reply.isError() || service != m_current)
            return;
        const double pos = reply.value().variant().toLongLong() / 1000.0;
        const double predicted = m_position + (playing() ? double(nowMs()) - m_positionStamp : 0.0);
        // ignora tremidinhas de poucos ms para a barra não "pular"
        if (std::abs(pos - predicted) < 350 && playing())
            return;
        m_position = pos;
        m_positionStamp = double(nowMs());
        emit positionChanged();
    });
}

// ---------------------------------------------------------------- capa

void Media::loadArt()
{
    const Player *p = cur();
    QString url = p ? p->meta.value(QStringLiteral("mpris:artUrl")).toString() : QString();
    url.replace(QLatin1String("https://open.spotify.com/image/"), QLatin1String("https://i.scdn.co/image/"));
    if (url == m_artUrl)
        return;
    m_artUrl = url;

    if (url.isEmpty()) {
        setArtImage(QString(), QImage());
        return;
    }
    const QUrl qurl(url);
    if (qurl.isLocalFile()) {
        setArtImage(qurl.toString(), QImage(qurl.toLocalFile()));
        return;
    }
    if (qurl.scheme() != QLatin1String("http") && qurl.scheme() != QLatin1String("https"))
        return;

    const QString dir = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation) + QStringLiteral("/foco-max/art");
    QDir().mkpath(dir);
    const QString file = dir + QLatin1Char('/')
        + QString::fromLatin1(QCryptographicHash::hash(url.toUtf8(), QCryptographicHash::Sha1).toHex()) + QStringLiteral(".img");
    if (QFile::exists(file)) {
        setArtImage(QUrl::fromLocalFile(file).toString(), QImage(file));
        return;
    }
    QNetworkReply *reply = m_net->get(QNetworkRequest(qurl));
    connect(reply, &QNetworkReply::finished, this, [this, reply, url, file] {
        reply->deleteLater();
        if (reply->error() != QNetworkReply::NoError || url != m_artUrl)
            return;
        const QByteArray data = reply->readAll();
        QImage img;
        if (!img.loadFromData(data))
            return;
        QFile out(file);
        if (out.open(QIODevice::WriteOnly))
            out.write(data);
        out.close();
        setArtImage(QUrl::fromLocalFile(file).toString(), img);
    });
}

void Media::setArtImage(const QString &file, const QImage &img)
{
    m_art = file;
    pickColors(img, m_color, m_color2);
    emit artChanged();
}

// ---------------------------------------------------------------- análise de áudio

void Media::updateAnalyzer()
{
    const bool want = m_enabled && m_visualize && playing();
    if (want && m_capture.state() == QProcess::NotRunning)
        startCapture();
    else if (!want && m_capture.state() != QProcess::NotRunning)
        stopCapture();
    if (!want && !m_frame.isActive() && m_level > 0.001)
        m_frame.start(); // deixa as barras descerem suavemente
}

QString Media::captureTarget() const
{
    const Player *p = cur();
    if (!p || m_streamBroken)
        return QStringLiteral("--device=@DEFAULT_MONITOR@");
    // Grava só o som do player (sem notificações e outros apps), quando dá para achar o stream dele.
    QProcess pactl;
    pactl.start(QStringLiteral("pactl"), {QStringLiteral("-f"), QStringLiteral("json"), QStringLiteral("list"), QStringLiteral("sink-inputs")});
    if (pactl.waitForFinished(800)) {
        const QJsonArray inputs = QJsonDocument::fromJson(pactl.readAllStandardOutput()).array();
        const QString hint = (p->desktopEntry.isEmpty() ? p->service.mid(kPrefix.size()).section(QLatin1Char('.'), 0, 0) : p->desktopEntry).toLower();
        int byName = -1;
        for (const QJsonValue &v : inputs) {
            const QJsonObject o = v.toObject();
            if (o.value(QStringLiteral("corked")).toBool())
                continue;
            const QJsonObject props = o.value(QStringLiteral("properties")).toObject();
            const int index = o.value(QStringLiteral("index")).toInt(-1);
            const uint pid = props.value(QStringLiteral("application.process.id")).toString().toUInt();
            if (p->pid && pid && (isAncestor(p->pid, pid) || isAncestor(pid, p->pid)))
                return QStringLiteral("--monitor-stream=%1").arg(index);
            const QString name = (props.value(QStringLiteral("application.name")).toString() + QLatin1Char(' ')
                                  + props.value(QStringLiteral("application.process.binary")).toString()).toLower();
            if (!hint.isEmpty() && name.contains(hint))
                byName = index;
        }
        if (byName >= 0)
            return QStringLiteral("--monitor-stream=%1").arg(byName);
    }
    return QStringLiteral("--device=@DEFAULT_MONITOR@");
}

void Media::startCapture()
{
    m_captureArgs = captureTarget();
    m_captureClock.start();
    m_capture.start(QStringLiteral("parec"), {m_captureArgs, QStringLiteral("--format=float32le"), QStringLiteral("--rate=%1").arg(kRate),
                                              QStringLiteral("--channels=1"), QStringLiteral("--latency-msec=20"),
                                              QStringLiteral("--client-name=Foquinho"), QStringLiteral("--stream-name=ondas")});
    m_frame.start();
    m_retarget.start();
}

void Media::stopCapture()
{
    m_retarget.stop();
    if (m_capture.state() == QProcess::NotRunning)
        return;
    m_capture.blockSignals(true);
    m_capture.kill();
    m_capture.waitForFinished(500);
    m_capture.blockSignals(false);
    m_capture.readAllStandardOutput();
    std::fill(m_ring.begin(), m_ring.end(), 0.0f);
}

void Media::readAudio()
{
    static QByteArray rest;
    rest += m_capture.readAllStandardOutput();
    const int count = int(rest.size() / sizeof(float));
    const float *samples = reinterpret_cast<const float *>(rest.constData());
    for (int i = 0; i < count; ++i) {
        m_ring[m_ringPos] = samples[i];
        m_ringPos = (m_ringPos + 1) % kRing;
    }
    rest.remove(0, int(count * sizeof(float)));
}

void Media::analyze()
{
    const bool live = m_capture.state() == QProcess::Running;
    std::vector<std::complex<float>> buf(kFft);
    if (live) {
        for (int i = 0; i < kFft; ++i) {
            const float hann = 0.5f - 0.5f * std::cos(2.0f * float(M_PI) * i / (kFft - 1));
            buf[i] = m_ring[(m_ringPos - kFft + i + kRing) % kRing] * hann;
        }
        fft(buf);
    }

    // bandas logarítmicas de ~45 Hz a ~14 kHz
    const double binHz = double(kRate) / kFft;
    float raw[kBands];
    float frameMax = 0;
    int prevHi = 0;
    for (int b = 0; b < kBands; ++b) {
        const double f0 = 45.0 * std::pow(14000.0 / 45.0, double(b) / kBands);
        const double f1 = 45.0 * std::pow(14000.0 / 45.0, double(b + 1) / kBands);
        int lo = std::max(prevHi, int(f0 / binHz));
        int hi = std::max(lo + 1, int(f1 / binHz));
        prevHi = hi;
        float sum = 0;
        for (int k = lo; k < hi && k < kFft / 2; ++k)
            sum += live ? std::norm(buf[k]) : 0.0f;
        // compensa a queda natural de energia nos agudos
        raw[b] = std::sqrt(sum / float(hi - lo)) * (1.0f + b * 0.11f);
        frameMax = std::max(frameMax, raw[b]);
    }

    m_gain = std::max({frameMax, m_gain * 0.996f, 0.01f});
    float sumLevel = 0;
    for (int b = 0; b < kBands; ++b) {
        m_peak[b] = std::max(raw[b], m_peak[b] * 0.992f);
        const float ref = std::max(m_peak[b], m_gain * 0.35f);
        float v = live ? std::clamp(raw[b] / ref, 0.0f, 1.0f) : 0.0f;
        v = std::pow(v, 1.4f); // realça os picos para as ondas "baterem"
        m_bands[b] = v > m_bands[b] ? m_bands[b] + (v - m_bands[b]) * 0.65f : m_bands[b] * 0.80f + v * 0.20f;
        sumLevel += m_bands[b];
        m_levelsOut[b] = double(m_bands[b]);
    }
    m_level = sumLevel / kBands;

    // batida: energia dos graves acima da média recente
    const float bass = (raw[0] + raw[1] + raw[2] + raw[3]) / 4.0f;
    const bool hit = live && bass > m_bassAvg * 1.45f && bass > m_gain * 0.25f && m_sinceBeat.elapsed() > 230;
    m_bassAvg = m_bassAvg * 0.93f + bass * 0.07f;
    if (hit) {
        m_beat = 1.0;
        m_sinceBeat.restart();
    } else {
        m_beat *= 0.86;
    }
    emit levelsChanged();

    if (!live && m_level < 0.002 && m_beat < 0.01) {
        m_level = 0;
        m_beat = 0;
        m_frame.stop();
        emit levelsChanged();
    }
}
