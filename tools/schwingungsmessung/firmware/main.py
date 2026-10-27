# ADXL345 am RP2040 → USB, für die Schwingungsmessung am Lasercutter.
#
# MicroPython-Programm für einen Raspberry Pi Pico mit angeschlossenem
# ADXL345 oder für fertige USB-C-Sensorboards mit RP2040 (Mellow FLY
# ADXL345 USB, BIGTREETECH ADXL345 V2, FYSETC Portable Input Shaper). Als
# `main.py` auf das Board kopieren; es startet dann bei jedem Einstecken.
#
# Befehle über die USB-Schnittstelle (eine Zeile, abgeschlossen mit \n):
#   I        Info-Zeile ausgeben
#   S<Rate>  Messung mit 3200, 1600, 800 oder 400 Hz starten, z. B. S3200
#   X        (oder jede andere Eingabe) Messung beenden
#
# Nach `S` folgt die Zeile `OK ADXL345 rate=<Hz> preset=<Name>`, danach
# Binärrahmen: A5 5A, Anzahl n (1 Byte), Flags (1 Byte, Bit 0: FIFO war
# voll, Werte können fehlen), ticks_us nach dem Auslesen (uint32 LE), dann
# n × 7 Bytes: 1 Füllbyte und X, Y, Z als int16 LE (3,9 mg je Stufe).

import select
import struct
import sys
import time

import micropython
from machine import SPI, Pin, SoftSPI

# (Name, SPI-Bus, SCK, MOSI/SDA, MISO/SDO, CS). Die Boards werden in dieser
# Reihenfolge ausprobiert, je erst mit Hardware-, dann mit Software-SPI;
# gefunden ist der Sensor, wenn er sich mit seiner Kennung 0xE5 meldet. Für
# ein anderes Board die Pins aus dessen Klipper-Konfiguration ergänzen.
PRESETS = (
    ("mellow-btt", 1, 10, 11, 12, 9),  # Mellow FLY ADXL345 USB, BTT ADXL345 V2
    ("fysetc", 1, 10, 11, 12, 13),  # FYSETC Portable Input Shaper
    ("pico", 0, 2, 3, 0, 1),  # Pico: SCL→GP2, SDA→GP3, SDO→GP0, CS→GP1
)
# Fester Name aus PRESETS statt Ausprobieren, z. B. "pico".
PRESET = None

RATES = {3200: 0x0F, 1600: 0x0E, 800: 0x0D, 400: 0x0C}

DEVID = 0x00
BW_RATE = 0x2C
POWER_CTL = 0x2D
INT_ENABLE = 0x2E
DATA_FORMAT = 0x31
DATAX0 = 0x32
FIFO_CTL = 0x38
FIFO_STATUS = 0x39

READ = 0x80
MULTI = 0x40
FIFO_SIZE = 32
HEADER = 8
SLOT = 7


class Sensor:
    def __init__(self, preset, soft):
        self.name, bus, sck, mosi, miso, cs = preset
        self.cs = Pin(cs, Pin.OUT, value=1)
        # SPI-Modus 3, höchstens 5 MHz; 3200 Hz brauchen mindestens 2 MHz.
        # Software-SPI für Boards, deren Pins nicht zur Hardware passen.
        pins = {"sck": Pin(sck), "mosi": Pin(mosi), "miso": Pin(miso)}
        if soft:
            self.name += "-soft"
            self.spi = SoftSPI(baudrate=5_000_000, polarity=1, phase=1, **pins)
        else:
            self.spi = SPI(bus, baudrate=5_000_000, polarity=1, phase=1, **pins)
        self.tx = bytearray(2)
        self.rx = bytearray(2)

    def read(self, register):
        self.tx[0] = READ | register
        self.tx[1] = 0
        self.cs(0)
        self.spi.write_readinto(self.tx, self.rx)
        self.cs(1)
        return self.rx[1]

    def write(self, register, value):
        self.tx[0] = register
        self.tx[1] = value
        self.cs(0)
        self.spi.write(self.tx)
        self.cs(1)

    def release(self):
        self.spi.deinit()
        self.cs.init(Pin.IN)

    def start(self, rate):
        self.write(POWER_CTL, 0x00)
        self.write(INT_ENABLE, 0x00)
        # Volle Auflösung (3,9 mg je Stufe), ±16 g.
        self.write(DATA_FORMAT, 0x0B)
        self.write(BW_RATE, RATES[rate])
        # FIFO leeren (Bypass), dann Stream-Modus.
        self.write(FIFO_CTL, 0x00)
        self.write(FIFO_CTL, 0x80)
        self.write(POWER_CTL, 0x08)

    def stop(self):
        self.write(POWER_CTL, 0x00)


def find():
    for preset in PRESETS:
        if PRESET is not None and preset[0] != PRESET:
            continue
        for soft in (False, True):
            sensor = Sensor(preset, soft)
            if sensor.read(DEVID) == 0xE5 and sensor.read(DEVID) == 0xE5:
                return sensor
            sensor.release()
    return None


def stream(sensor, out, poll):
    # Alles vorab anlegen: in der Schleife wird nichts alloziert, damit keine
    # Speicherbereinigung den FIFO (32 Werte = 10 ms bei 3200 Hz) überlaufen lässt.
    frame = bytearray(HEADER + FIFO_SIZE * SLOT)
    frame[0] = 0xA5
    frame[1] = 0x5A
    view = memoryview(frame)
    slots = [view[HEADER + SLOT * i : HEADER + SLOT * (i + 1)] for i in range(FIFO_SIZE)]
    frames = [view[: HEADER + SLOT * n] for n in range(FIFO_SIZE + 1)]
    data_tx = bytearray(SLOT)
    data_tx[0] = READ | MULTI | DATAX0
    status_tx = bytearray((READ | FIFO_STATUS, 0))
    status_rx = bytearray(2)
    spi = sensor.spi
    cs = sensor.cs
    while True:
        for _ in poll.ipoll(0):
            return
        cs(0)
        spi.write_readinto(status_tx, status_rx)
        cs(1)
        n = status_rx[1] & 0x3F
        if n < 8:
            time.sleep_us(300)
            continue
        flags = 1 if n >= FIFO_SIZE else 0
        for i in range(n):
            cs(0)
            spi.write_readinto(data_tx, slots[i])
            cs(1)
        struct.pack_into("<BBI", frame, 2, n, flags, time.ticks_us())
        out.write(frames[n])


def main():
    micropython.alloc_emergency_exception_buf(100)
    sensor = find()
    poll = select.poll()
    poll.register(sys.stdin, select.POLLIN)
    while True:
        line = sys.stdin.readline().strip()
        if not line:
            continue
        if sensor is None:
            sensor = find()
        if sensor is None:
            print("ERR ADXL345 nicht gefunden (Verdrahtung oder PRESETS prüfen)")
            continue
        if line == "I":
            print("INFO ADXL345 preset=%s" % sensor.name)
        elif line[0] == "S":
            rate = int(line[1:] or 3200)
            if rate not in RATES:
                print("ERR Rate %d nicht unterstützt" % rate)
                continue
            sensor.start(rate)
            print("OK ADXL345 rate=%d preset=%s" % (rate, sensor.name))
            try:
                stream(sensor, sys.stdout.buffer, poll)
            finally:
                sensor.stop()
            # Den Stopp-Befehl verwerfen.
            sys.stdin.readline()


main()
