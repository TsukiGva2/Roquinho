#pragma once

#include <QColor>
#include <QDBusContext>
#include <QElapsedTimer>
#include <QHash>
#include <QObject>
#include <QProcess>
#include <QTimer>
#include <QVariant>
#include <QVector>

class QNetworkAccessManager;
class QDBusInterface;

// Player de música do PC via MPRIS (Spotify em primeiro lugar).
// Só considera música: players de vídeo, abas de navegador fora de sites de música
// e "players" falsos (ex.: o WhatsApp Web tocando o som de notificação) são ignorados.
// Também analisa o áudio que está saindo (parec) para as ondinhas seguirem a batida.
class Media : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(bool active READ active NOTIFY trackChanged)
    Q_PROPERTY(bool playing READ playing NOTIFY playingChanged)
    Q_PROPERTY(QString title READ title NOTIFY trackChanged)
    Q_PROPERTY(QString artist READ artist NOTIFY trackChanged)
    Q_PROPERTY(QString album READ album NOTIFY trackChanged)
    Q_PROPERTY(QString player READ player NOTIFY trackChanged)
    Q_PROPERTY(QString art READ art NOTIFY artChanged)
    Q_PROPERTY(QColor color READ color NOTIFY artChanged)
    Q_PROPERTY(QColor color2 READ color2 NOTIFY artChanged)
    Q_PROPERTY(double length READ length NOTIFY trackChanged)
    Q_PROPERTY(double position READ position NOTIFY positionChanged)
    Q_PROPERTY(double positionStamp READ positionStamp NOTIFY positionChanged)
    Q_PROPERTY(bool canNext READ canNext NOTIFY capsChanged)
    Q_PROPERTY(bool canPrevious READ canPrevious NOTIFY capsChanged)
    Q_PROPERTY(bool canSeek READ canSeek NOTIFY capsChanged)
    Q_PROPERTY(bool visualize READ visualize WRITE setVisualize NOTIFY visualizeChanged)
    Q_PROPERTY(QVariantList levels READ levels NOTIFY levelsChanged)
    Q_PROPERTY(double level READ level NOTIFY levelsChanged)
    Q_PROPERTY(double beat READ beat NOTIFY levelsChanged)

public:
    explicit Media(QObject *parent = nullptr);
    ~Media() override;

    bool enabled() const { return m_enabled; }
    void setEnabled(bool on);
    bool active() const { return m_enabled && !m_current.isEmpty(); }
    bool playing() const;
    QString title() const;
    QString artist() const;
    QString album() const;
    QString player() const;
    QString art() const { return active() ? m_art : QString(); }
    QColor color() const { return m_color; }
    QColor color2() const { return m_color2; }
    double length() const;
    double position() const { return m_position; }
    double positionStamp() const { return m_positionStamp; }
    bool canNext() const;
    bool canPrevious() const;
    bool canSeek() const;
    bool visualize() const { return m_visualize; }
    void setVisualize(bool on);
    QVariantList levels() const { return m_levelsOut; }
    double level() const { return m_level; }
    double beat() const { return m_beat; }

    Q_INVOKABLE void playPause();
    Q_INVOKABLE void next();
    Q_INVOKABLE void previous();
    Q_INVOKABLE void seek(double ms);
    Q_INVOKABLE void raise();

signals:
    void enabledChanged();
    void trackChanged();
    void playingChanged();
    void artChanged();
    void positionChanged();
    void capsChanged();
    void visualizeChanged();
    void levelsChanged();
    // Música nova começou (para o aviso no notch).
    void newTrack();

private slots:
    void onNameOwnerChanged(const QString &name, const QString &oldOwner, const QString &newOwner);
    void onPropertiesChanged(const QString &iface, const QVariantMap &changed, const QStringList &invalidated);
    void onSeeked(qlonglong us);

private:
    struct Player {
        QString service;      // org.mpris.MediaPlayer2.spotify
        QString owner;        // :1.123
        QString identity;
        QString desktopEntry;
        QString status;       // Playing | Paused | Stopped
        QVariantMap meta;
        bool canNext = false, canPrevious = false, canSeek = false, canRaise = false;
        qint64 lastActive = 0;
        uint pid = 0;
    };

    void addPlayer(const QString &service, const QString &owner);
    void removePlayer(const QString &service);
    void applyPlayerProps(Player &p, const QVariantMap &props);
    bool isMusic(const Player &p) const;
    void choose();
    const Player *cur() const;
    void call(const QString &method, const QVariantList &args = {});
    void pollPosition();
    void loadArt();
    void setArtImage(const QString &file, const QImage &img);

    // análise de áudio
    void updateAnalyzer();
    void startCapture();
    void stopCapture();
    QString captureTarget() const;
    void readAudio();
    void analyze();

    bool m_enabled = true;
    bool m_visualize = false;
    QHash<QString, Player> m_players; // por service
    QString m_current;
    QString m_lastTrackKey;
    double m_position = 0;
    double m_positionStamp = 0;
    QTimer m_positionPoll;

    QNetworkAccessManager *m_net = nullptr;
    QString m_artUrl;
    QString m_art;
    QColor m_color;
    QColor m_color2;

    QProcess m_capture;
    QString m_captureArgs;
    QElapsedTimer m_captureClock;
    bool m_streamBroken = false; // --monitor-stream falhou: usa o monitor da saída inteira
    QTimer m_frame;
    QTimer m_retarget;
    QVector<float> m_ring;
    int m_ringPos = 0;
    QVector<float> m_bands;
    QVector<float> m_peak;
    float m_gain = 0.02f;
    float m_bassAvg = 0;
    QElapsedTimer m_sinceBeat;
    QVariantList m_levelsOut;
    double m_level = 0;
    double m_beat = 0;
};
