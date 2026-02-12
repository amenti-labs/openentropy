"""Entropy source implementations."""

from esoteric_entropy.sources.audio import AudioNoiseSource
from esoteric_entropy.sources.base import EntropySource
from esoteric_entropy.sources.bluetooth import BluetoothNoiseSource
from esoteric_entropy.sources.camera import CameraNoiseSource
from esoteric_entropy.sources.compression import CompressionTimingSource, HashTimingSource
from esoteric_entropy.sources.novel import (
    DispatchQueueSource,
    DyldTimingSource,
    SpotlightTimingSource,
    VMPageTimingSource,
)
from esoteric_entropy.sources.cross_domain import (
    CPUIOBeatSource,
    CPUMemoryBeatSource,
    MultiDomainBeatSource,
)
from esoteric_entropy.sources.disk import DiskIOSource
from esoteric_entropy.sources.gpu import GPUTimingSource
from esoteric_entropy.sources.ioregistry import IORegistryEntropySource
from esoteric_entropy.sources.memory import MemoryTimingSource
from esoteric_entropy.sources.network import DNSTimingSource, TCPConnectSource
from esoteric_entropy.sources.process import ProcessSource
from esoteric_entropy.sources.sensor import SensorNoiseSource
from esoteric_entropy.sources.silicon import (
    CacheContentionSource,
    DRAMRowBufferSource,
    PageFaultTimingSource,
    SpeculativeExecutionSource,
)
from esoteric_entropy.sources.sysctl import SysctlSource
from esoteric_entropy.sources.timing import (
    ClockJitterSource,
    MachTimingSource,
    SleepJitterSource,
)
from esoteric_entropy.sources.vmstat import VmstatSource
from esoteric_entropy.sources.wifi import WiFiRSSISource

ALL_SOURCES: list[type[EntropySource]] = [
    # Original sources
    ClockJitterSource,
    MachTimingSource,
    SleepJitterSource,
    SysctlSource,
    VmstatSource,
    DNSTimingSource,
    TCPConnectSource,
    WiFiRSSISource,
    DiskIOSource,
    MemoryTimingSource,
    GPUTimingSource,
    ProcessSource,
    AudioNoiseSource,
    CameraNoiseSource,
    SensorNoiseSource,
    BluetoothNoiseSource,
    # New: Silicon-level
    DRAMRowBufferSource,
    CacheContentionSource,
    PageFaultTimingSource,
    SpeculativeExecutionSource,
    # New: IORegistry deep mining
    IORegistryEntropySource,
    # New: Cross-domain beat frequency
    CPUIOBeatSource,
    CPUMemoryBeatSource,
    MultiDomainBeatSource,
    # New: Compression/hash timing oracle
    CompressionTimingSource,
    HashTimingSource,
    # New: Novel discovered sources
    DispatchQueueSource,
    DyldTimingSource,
    VMPageTimingSource,
    SpotlightTimingSource,
]

__all__ = ["EntropySource", "ALL_SOURCES"]
