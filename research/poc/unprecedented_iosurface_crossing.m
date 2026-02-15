// IOSurface GPU/CPU Memory Domain Crossing — Multi-clock-domain coherence entropy
//
// IOSurface is shared memory between GPU and CPU. Writing from one domain and
// reading from another crosses multiple clock boundaries:
//   CPU -> fabric -> GPU memory controller -> GPU cache
//
// Each domain transition adds independent timing noise from cache coherence
// traffic, fabric arbitration, and cross-clock-domain synchronization.
//
// Build: cc -O2 -o unprecedented_iosurface_crossing unprecedented_iosurface_crossing.m -framework IOSurface -framework Metal -framework Foundation -framework CoreGraphics -lm

#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>
#import <Foundation/Foundation.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>

#define N_SAMPLES 12000
#define SURFACE_WIDTH 256
#define SURFACE_HEIGHT 256

static void analyze_entropy(const char *label, const uint8_t *data, int n) {
    int hist[256] = {0};
    for (int i = 0; i < n; i++) hist[data[i]]++;

    double shannon = 0.0;
    int max_count = 0, unique = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            unique++;
            if (hist[i] > max_count) max_count = hist[i];
            double p = (double)hist[i] / n;
            shannon -= p * log2(p);
        }
    }
    double min_entropy = -log2((double)max_count / n);
    printf("  %s: Shannon=%.3f  H∞=%.3f  unique=%d/256  n=%d\n",
           label, shannon, min_entropy, unique, n);
}

// Metal shader: write to shared texture then read back
static NSString *shaderSource = @""
"#include <metal_stdlib>\n"
"using namespace metal;\n"
"\n"
"kernel void write_texture(\n"
"    texture2d<float, access::write> tex [[texture(0)]],\n"
"    device uint *counter [[buffer(0)]],\n"
"    uint2 gid [[thread_position_in_grid]]\n"
") {\n"
"    uint val = atomic_fetch_add_explicit(\n"
"        (device atomic_uint *)counter, 1, memory_order_relaxed);\n"
"    float4 color = float4(\n"
"        float(val & 0xFF) / 255.0,\n"
"        float((val >> 8) & 0xFF) / 255.0,\n"
"        float((val >> 16) & 0xFF) / 255.0,\n"
"        1.0\n"
"    );\n"
"    tex.write(color, gid);\n"
"}\n"
"\n"
"kernel void read_texture(\n"
"    texture2d<float, access::read> tex [[texture(0)]],\n"
"    device uint *output [[buffer(0)]],\n"
"    uint2 gid [[thread_position_in_grid]]\n"
") {\n"
"    float4 color = tex.read(gid);\n"
"    uint val = uint(color.r * 255.0) ^ (uint(color.g * 255.0) << 8);\n"
"    output[gid.y * 256 + gid.x] = val;\n"
"}\n";

int main(void) {
    @autoreleasepool {
        printf("# IOSurface GPU/CPU Memory Domain Crossing — Coherence Entropy\n\n");

        id<MTLDevice> gpu = MTLCreateSystemDefaultDevice();
        if (!gpu) {
            printf("FAIL: No Metal device\n");
            return 1;
        }
        printf("GPU: %s\n", [[gpu name] UTF8String]);

        id<MTLCommandQueue> queue = [gpu newCommandQueue];

        // Create IOSurface — shared between CPU and GPU
        NSDictionary *props = @{
            (id)kIOSurfaceWidth: @(SURFACE_WIDTH),
            (id)kIOSurfaceHeight: @(SURFACE_HEIGHT),
            (id)kIOSurfaceBytesPerElement: @4,
            (id)kIOSurfacePixelFormat: @((uint32_t)'BGRA'),
        };
        IOSurfaceRef surface = IOSurfaceCreate((CFDictionaryRef)props);
        if (!surface) {
            printf("FAIL: Cannot create IOSurface\n");
            return 1;
        }
        printf("IOSurface created: %dx%d, %zu bytes\n\n",
               SURFACE_WIDTH, SURFACE_HEIGHT, IOSurfaceGetAllocSize(surface));

        // Create Metal texture backed by IOSurface
        MTLTextureDescriptor *texDesc = [MTLTextureDescriptor
            texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm
            width:SURFACE_WIDTH height:SURFACE_HEIGHT mipmapped:NO];
        texDesc.usage = MTLTextureUsageShaderRead | MTLTextureUsageShaderWrite;

        id<MTLTexture> texture = [gpu newTextureWithDescriptor:texDesc
                                                     iosurface:surface
                                                         plane:0];

        // Compile shaders
        NSError *error = nil;
        id<MTLLibrary> library = [gpu newLibraryWithSource:shaderSource
                                                   options:nil error:&error];
        if (!library) {
            printf("FAIL: Shader compile: %s\n", [[error localizedDescription] UTF8String]);
            CFRelease(surface);
            return 1;
        }

        id<MTLComputePipelineState> writePipeline =
            [gpu newComputePipelineStateWithFunction:
                [library newFunctionWithName:@"write_texture"] error:&error];
        id<MTLComputePipelineState> readPipeline =
            [gpu newComputePipelineStateWithFunction:
                [library newFunctionWithName:@"read_texture"] error:&error];

        id<MTLBuffer> counterBuf = [gpu newBufferWithLength:sizeof(uint32_t)
                                                    options:MTLResourceStorageModeShared];
        uint32_t output_size = SURFACE_WIDTH * SURFACE_HEIGHT * sizeof(uint32_t);
        id<MTLBuffer> outputBuf = [gpu newBufferWithLength:output_size
                                                   options:MTLResourceStorageModeShared];

        uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
        uint8_t *lsbs = malloc(N_SAMPLES);

        // === Test 1: CPU write → GPU read timing ===
        printf("--- Test 1: CPU Write → GPU Read Timing ---\n");
        for (int i = 0; i < N_SAMPLES; i++) {
            // CPU writes to IOSurface
            IOSurfaceLock(surface, 0, NULL);
            uint8_t *base = (uint8_t *)IOSurfaceGetBaseAddress(surface);
            // Write a pattern that varies each iteration
            uint32_t pattern = (uint32_t)(mach_absolute_time() & 0xFFFFFFFF);
            memset(base, (uint8_t)(pattern & 0xFF), 64); // Write just first cache line
            IOSurfaceUnlock(surface, 0, NULL);

            // GPU reads the texture
            memset([counterBuf contents], 0, sizeof(uint32_t));
            uint64_t t0 = mach_absolute_time();
            id<MTLCommandBuffer> cmdBuf = [queue commandBuffer];
            id<MTLComputeCommandEncoder> enc = [cmdBuf computeCommandEncoder];
            [enc setComputePipelineState:readPipeline];
            [enc setTexture:texture atIndex:0];
            [enc setBuffer:outputBuf offset:0 atIndex:0];
            [enc dispatchThreads:MTLSizeMake(SURFACE_WIDTH, SURFACE_HEIGHT, 1)
               threadsPerThreadgroup:MTLSizeMake(16, 16, 1)];
            [enc endEncoding];
            [cmdBuf commit];
            [cmdBuf waitUntilCompleted];
            uint64_t t1 = mach_absolute_time();

            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
        analyze_entropy("CPU→GPU crossing LSBs", lsbs, N_SAMPLES);

        // === Test 2: GPU write → CPU read timing ===
        printf("\n--- Test 2: GPU Write → CPU Read Timing ---\n");
        for (int i = 0; i < N_SAMPLES; i++) {
            // GPU writes to texture
            memset([counterBuf contents], 0, sizeof(uint32_t));
            id<MTLCommandBuffer> cmdBuf = [queue commandBuffer];
            id<MTLComputeCommandEncoder> enc = [cmdBuf computeCommandEncoder];
            [enc setComputePipelineState:writePipeline];
            [enc setTexture:texture atIndex:0];
            [enc setBuffer:counterBuf offset:0 atIndex:0];
            [enc dispatchThreads:MTLSizeMake(SURFACE_WIDTH, SURFACE_HEIGHT, 1)
               threadsPerThreadgroup:MTLSizeMake(16, 16, 1)];
            [enc endEncoding];
            [cmdBuf commit];
            [cmdBuf waitUntilCompleted];

            // CPU reads from IOSurface — this crosses GPU→CPU coherence boundary
            uint64_t t0 = mach_absolute_time();
            IOSurfaceLock(surface, kIOSurfaceLockReadOnly, NULL);
            volatile uint8_t *base = (volatile uint8_t *)IOSurfaceGetBaseAddress(surface);
            volatile uint8_t sum = 0;
            for (int j = 0; j < 64; j++) sum ^= base[j]; // Read first cache line
            IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, NULL);
            uint64_t t1 = mach_absolute_time();

            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        tmin = timings[0]; tmax = timings[0]; tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
        analyze_entropy("GPU→CPU crossing LSBs", lsbs, N_SAMPLES);

        // === Test 3: Round-trip (CPU→GPU→CPU) timing ===
        printf("\n--- Test 3: Round-Trip CPU→GPU→CPU Timing ---\n");
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = mach_absolute_time();

            // CPU write
            IOSurfaceLock(surface, 0, NULL);
            uint8_t *wbase = (uint8_t *)IOSurfaceGetBaseAddress(surface);
            wbase[0] = (uint8_t)(i & 0xFF);
            IOSurfaceUnlock(surface, 0, NULL);

            // GPU process
            memset([counterBuf contents], 0, sizeof(uint32_t));
            id<MTLCommandBuffer> cmdBuf = [queue commandBuffer];
            id<MTLComputeCommandEncoder> enc = [cmdBuf computeCommandEncoder];
            [enc setComputePipelineState:writePipeline];
            [enc setTexture:texture atIndex:0];
            [enc setBuffer:counterBuf offset:0 atIndex:0];
            [enc dispatchThreads:MTLSizeMake(16, 16, 1)
               threadsPerThreadgroup:MTLSizeMake(16, 16, 1)];
            [enc endEncoding];
            [cmdBuf commit];
            [cmdBuf waitUntilCompleted];

            // CPU read
            IOSurfaceLock(surface, kIOSurfaceLockReadOnly, NULL);
            volatile uint8_t *rbase = (volatile uint8_t *)IOSurfaceGetBaseAddress(surface);
            volatile uint8_t val = rbase[0];
            (void)val;
            IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, NULL);

            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        tmin = timings[0]; tmax = timings[0]; tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
        analyze_entropy("Round-trip crossing LSBs", lsbs, N_SAMPLES);

        // === Test 4: Delta timing ===
        printf("\n--- Test 4: Round-Trip Delta ---\n");
        uint8_t *deltas = malloc(N_SAMPLES);
        for (int i = 1; i < N_SAMPLES; i++) {
            int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
            uint64_t ud = (uint64_t)d;
            deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
        }
        analyze_entropy("Round-trip delta XOR-fold", deltas, N_SAMPLES - 1);

        free(timings);
        free(lsbs);
        free(deltas);
        CFRelease(surface);

        printf("\nDone.\n");
    }
    return 0;
}
