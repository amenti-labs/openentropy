"""Entropy source implementations."""

from esoteric_entropy.sources.audio import AudioNoiseSource
from esoteric_entropy.sources.base import EntropySource
from esoteric_entropy.sources.bluetooth import BluetoothNoiseSource
from esoteric_entropy.sources.camera import CameraNoiseSource
from esoteric_entropy.sources.disk import DiskIOSource
from esoteric_entropy.sources.gpu import GPUTimingSource
from esoteric_entropy.sources.memory import MemoryTimingSource
from esoteric_entropy.sources.network import DNSTimingSource, TCPConnectSource
from esoteric_entropy.sources.process import ProcessSource
from esoteric_entropy.sources.sensor import SensorNoiseSource
from esoteric_entropy.sources.sysctl import SysctlSource
from esoteric_entropy.sources.timing import (
    ClockJitterSource,
    MachTimingSource,
    SleepJitterSource,
)
from esoteric_entropy.sources.vmstat import VmstatSource

ALL_SOURCES: list[type[EntropySource]] = [
    ClockJitterSource,
    MachTimingSource,
    SleepJitterSource,
    SysctlSource,
    VmstatSource,
    DNSTimingSource,
    TCPConnectSource,
    DiskIOSource,
    MemoryTimingSource,
    GPUTimingSource,
    ProcessSource,
    AudioNoiseSource,
    CameraNoiseSource,
    SensorNoiseSource,
    BluetoothNoiseSource,
]

__all__ = ["EntropySource", "ALL_SOURCES"]
