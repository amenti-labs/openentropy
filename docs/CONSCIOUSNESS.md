# Consciousness-RNG Experiment Protocol

OpenEntropy's consciousness-RNG framework implements a rigorous multi-mode experimental protocol for testing whether focused human intention can influence hardware random number generators.

## Theoretical Background

### PEAR Lab Legacy (1979-2007)

The Princeton Engineering Anomalies Research (PEAR) Laboratory conducted 28 years of systematic experiments on human-machine anomalies. Their tripolar protocol (Baseline / High intention / Low intention) produced a cumulative effect size of ~0.0003 over millions of trials — tiny but statistically significant.

Key references:
- Jahn & Dunne (2005), *Consciousness and the Source of Reality*
- Nelson et al. (2002), "Correlations of continuous random data with major world events"
- Radin (2006), *Entangled Minds*

### Schmidt's Source-Independence Hypothesis

Helmut Schmidt (1970s-1990s) proposed that the consciousness-RNG effect should be independent of the physical mechanism generating randomness. Our **spectroscopy mode** directly tests this hypothesis by comparing effect sizes across source domains.

### OpenEntropy's Unique Contribution

Unlike any existing consciousness-RNG platform, OpenEntropy:
1. Runs trials independently across 40+ entropy sources simultaneously
2. Groups sources by physical domain for cross-mechanism comparison
3. Uses information-theoretic measures beyond simple mean shift
4. Tests inter-source coherence during intention epochs
5. Includes a deterministic PRNG as a built-in negative control
6. Temporal onset/decay analysis to detect *when* effects appear
7. ML-lite multivariate anomaly detection via Mahalanobis distance
8. Real-time feedback-guided intention training with learning curves
9. Two-operator adversarial protocol for competitive intention testing
10. Long-running entropic weather station for continuous monitoring
11. Operator profiling with cumulative cross-session statistics
12. Pre-registration with SHA-256 parameter hashing
13. Double-blind randomization of intention directions
14. Cross-session meta-analysis with forest plots

## Experiment Modes

### Standard Mode (default)

The classic PEAR Lab tripolar protocol.

**Protocol**: Three phases — Baseline, High intention (increase 1-bits), Low intention (decrease 1-bits). Each phase collects N trials of B bits, with SHA-256 conditioning ensuring the Binomial(n, 0.5) null hypothesis.

**Statistics**:
- Per-trial Z: `(observed_ones - n/2) / sqrt(n/4)`
- Cumulative Z: Stouffer's method — `sum(Z_i) / sqrt(N)`
- Differential Z: `(Z_high - Z_low) / sqrt(2)`
- Two-tailed p-values from normal CDF (Abramowitz & Stegun 26.2.17)

**Usage**:
```bash
openentropy consciousness --quick --interval 100
openentropy consciousness --trials 100 --bits 200 --output results.json
```

### Spectroscopy Mode

Cross-mechanism consciousness spectroscopy. Groups sources by physical domain (thermal, timing, sensor, microarch, IO, etc.) and tests whether the intention effect varies across domains.

**Hypothesis tested**: Schmidt's source-independence — if consciousness interacts with physical randomness, does it do so equally across all mechanism types?

**Statistics**:
- Per-domain differential Z (Stouffer aggregation within domain)
- Cochran's Q for heterogeneity across domains
- I-squared for percentage of variance due to heterogeneity
- Benjamini-Hochberg FDR correction for multiple domain comparisons

**Interpretation**:
- I-squared < 25%: Homogeneous — effect consistent across domains (supports Schmidt)
- I-squared 25-50%: Low heterogeneity
- I-squared 50-75%: Moderate heterogeneity — effect varies by domain
- I-squared > 75%: High heterogeneity — effect strongly domain-dependent (contradicts Schmidt)

**Usage**:
```bash
openentropy consciousness --mode spectroscopy --quick --interval 100
openentropy consciousness --mode spectroscopy --trials 100 --output spectroscopy.json
```

### Structure Mode

Information-theoretic signature detection. Measures whether intention epochs show altered information structure compared to baseline — not just mean shift, but changes in complexity, regularity, and spectral content.

**Measures**:
- **Approximate Entropy (ApEn)**: Regularity measure (Pincus 1991). Lower = more regular.
- **Sample Entropy (SampEn)**: Bias-corrected version (Richman & Moorman 2000).
- **LZ76 Complexity**: Lempel-Ziv normalized algorithmic complexity. 1.0 = maximally complex (random).
- **Spectral Flatness**: Wiener entropy. 1.0 = white noise, 0.0 = pure tone.

**Statistics**: Welch's t-test comparing intention epochs vs. baseline epochs for each measure.

**Interpretation**: If intention injects structure into random data (not just bias), we'd expect decreased ApEn/SampEn (more regularity), decreased LZ76 (less complexity), or decreased spectral flatness (spectral peaks).

**Usage**:
```bash
openentropy consciousness --mode structure --quick --interval 100
openentropy consciousness --mode structure --epochs 10 --epoch-duration 60
```

### Coherence Mode

Cross-source coherence analysis. Tests whether independent entropy sources become more correlated during intention epochs compared to baseline.

**Rationale**: If consciousness creates a "field effect" on hardware randomness, physically independent sources might show increased correlation during focused intention — even though they share no common physical mechanism.

**Statistics**:
- Pairwise Pearson correlations between all source pairs
- Fisher Z-transform for comparing correlations across epochs
- Stouffer combination of per-pair Fisher Z statistics
- Benjamini-Hochberg FDR correction for multiple pair comparisons

**Usage**:
```bash
openentropy consciousness --mode coherence --quick --interval 100
openentropy consciousness --mode coherence --epochs 8 --epoch-duration 30
```

### Temporal Mode

Temporal analysis of *when* during a trial block the consciousness effect appears, how quickly it onsets, and how it decays. PEAR Lab found effects were strongest in the first few seconds of intention.

**Analysis**:
- **Autocorrelation**: Tests whether Z-scores within a phase show temporal structure (non-independence)
- **Peak effect window**: Sliding window to find the trial range with maximum Stouffer Z
- **Onset detection**: CUSUM change-point detection for transition from null to effect
- **Decay analysis**: Exponential decay fit via log-linear regression with R-squared goodness-of-fit

**Usage**:
```bash
openentropy consciousness --mode temporal --quick --interval 100
openentropy consciousness --mode temporal --trials 100 --output temporal.json
```

### Adversarial Mode

Two-operator adversarial protocol where two people simultaneously focus opposing intentions on the same entropy sources.

**Protocol**: Operator A intends HIGH, Operator B intends LOW. The net effect Z tests whether one operator dominates. If intentions truly influence hardware, the stronger "will" should win.

**Statistics**:
- Net Z: Combined operator Z-scores (should cancel if equal strength)
- Dominance Z: |Z_A| - |Z_B| (positive means A is stronger)

**Usage**:
```bash
openentropy consciousness --mode adversarial --quick
openentropy consciousness --mode adversarial --trials 100 --output adversarial.json
```

### Feedback Mode

Real-time feedback-guided intention training. The operator sees a visual bar after each trial, providing immediate feedback on whether their intention shifted the bits.

**Measures**:
- **Learning correlation**: Pearson r between trial index and |Z| — positive means improvement over time
- **Learning t-test**: Welch's t-test comparing first-half vs second-half performance
- **First/second half mean Z**: Direct comparison of early vs late performance

**Usage**:
```bash
openentropy consciousness --mode feedback --quick --interval 200
openentropy consciousness --mode feedback --trials 200 --output feedback.json
```

### Anomaly Mode

ML-lite multivariate anomaly detection using Mahalanobis distance. Instead of testing specific hypotheses (mean shift, complexity change), extracts a 10-dimensional feature vector from each epoch and tests whether intention epochs are outliers from the baseline distribution.

**Features extracted**: mean, variance, skewness, kurtosis, bit_bias, approximate_entropy, lz76_complexity, spectral_flatness, max_run_length, mean_absolute_change.

**Statistics**:
- Baseline distribution estimation (mean vector + covariance matrix)
- Mahalanobis distance for each intention epoch
- Chi-squared threshold for anomaly detection (alpha=0.05)
- Gauss-Jordan matrix inversion with partial pivoting

**Usage**:
```bash
openentropy consciousness --mode anomaly --epochs 6 --epoch-duration 15 --quick
openentropy consciousness --mode anomaly --epochs 20 --epoch-duration 60 --output anomaly.json
```

## Additional Commands

### consciousness-meta (Meta-Analysis)

Cross-session meta-analysis. Reads multiple JSON result files and computes combined statistics.

**Features**:
- Combined Stouffer Z across all sessions
- ASCII forest plot showing per-session Z-scores
- Per-source cross-session analysis (which sources consistently respond)
- Interpretation with statistical significance assessment

**Usage**:
```bash
openentropy consciousness-meta session1.json session2.json session3.json
openentropy consciousness-meta *.json --output meta_analysis.json
```

### consciousness-weather (Entropic Weather Station)

Long-running entropy monitor that records periodic epochs with Z-scores and information-theoretic measures. Runs for hours or days, searching for temporal patterns and extreme events.

**Features**:
- Real-time display of per-epoch Z-scores with ASCII bar
- Spectral flatness and LZ76 complexity per epoch
- Auto-save every 100 epochs
- Extreme event detection (|Z| > 2)
- First-half vs second-half trend analysis

**Usage**:
```bash
openentropy consciousness-weather --interval 60 --output weather.json
openentropy consciousness-weather --duration 3600 --interval 30 --output hourly.json
```

### consciousness-profile (Operator Profiling)

View operator profiles that track session history, per-source responsiveness, and cumulative effect sizes across sessions.

**Features**:
- Session-by-session Z-score history
- Most responsive entropy sources ranked by effect size
- Cumulative Z trend visualization
- Pre-registration markers for rigorous sessions

**Usage**:
```bash
# Run an experiment with operator tracking
openentropy consciousness --operator alice --quick --output session1.json

# View accumulated profile
openentropy consciousness-profile alice
```

### consciousness-batch (Automated Session Batching)

Runs N consciousness experiment sessions back-to-back with automatic meta-analysis. Designed for accumulating statistical power over extended testing periods without manual intervention.

**Features**:
- Configurable number of sessions with rest periods between them
- Running Stouffer Z meta-analysis updated after each session
- ASCII forest plot of per-session Z-scores
- Per-source cross-session consistency analysis
- Automatic operator profile updates
- Per-session JSON files plus combined `meta_analysis.json`
- Ctrl+C graceful shutdown between sessions

**Usage**:
```bash
# Quick batch: 5 sessions with 10-second rest
openentropy consciousness-batch --sessions 5 --quick --rest 10 --output batch_results/

# Deep batch: 20 sessions for serious data collection
openentropy consciousness-batch --sessions 20 --trials 100 --rest 30 --operator alice --output deep_run/

# Overnight run with minimal rest
openentropy consciousness-batch --sessions 100 --quick --rest 5 --output overnight/
```

### consciousness-network (Networked Multi-Operator)

TCP-based networked adversarial protocol for remote multi-operator experiments. One machine acts as the entropy server, collecting data and coordinating the experiment. Remote operators connect and receive real-time phase instructions and feedback.

**Protocol**:
1. Host starts and waits for operator connections
2. Operators connect via TCP and send their name
3. Host assigns alternating High/Low intention directions
4. Host runs trials, broadcasts results to all connected operators
5. After all phases, host computes per-operator adversarial analysis

**Features**:
- Newline-delimited JSON wire protocol
- Real-time trial feedback for all connected operators
- Automatic direction assignment (alternating High/Low)
- Per-operator Z-score analysis
- Graceful handling of disconnections

**Usage**:
```bash
# Machine 1: Start as host
openentropy consciousness-network --host --port 9042 --quick

# Machine 2: Connect as operator
openentropy consciousness-network --connect 192.168.1.10:9042 --name alice

# Machine 3: Connect as second operator
openentropy consciousness-network --connect 192.168.1.10:9042 --name bob
```

## TUI Dashboard (`--tui`)

Launch a live terminal dashboard for any consciousness experiment with real-time Z-score visualization, PEAR Lab-style feedback bars, per-source heatmaps, and cumulative trend tracking.

**Views** (cycle with Tab):
- **Overview**: Z-score trace chart + feedback bar + source rankings
- **Source Heatmap**: Per-source Z-score heatmap across all trials (Unicode block characters)
- **Cumulative Trend**: Running cumulative Z with ±1.96 significance threshold lines
- **Phase Comparison**: Side-by-side phase Z-score bar chart

**Usage**:
```bash
# Standard mode with TUI
openentropy consciousness --tui --quick --interval 200

# Spectroscopy with TUI
openentropy consciousness --tui --mode spectroscopy --quick --interval 200
```

## Advanced Features

### ML Classification (`consciousness_ml` module)

Pure-Rust nearest-centroid classifier that distinguishes baseline from intention epochs based on 10-dimensional anomaly feature vectors. Available as a library module for programmatic use.

**Features**:
- 10-feature extraction: mean, variance, skewness, kurtosis, bit_bias, approximate_entropy, lz76_complexity, spectral_flatness, max_run_length, mean_absolute_change
- Fisher's discriminant ratio for feature importance ranking
- Leave-one-out cross-validation (LOOCV) accuracy
- Classification report with per-feature analysis

### Environmental Correlation (`consciousness_env` module)

GCP-style event correlation analysis. Records timestamps alongside Z-scores and tests whether entropy source behavior correlates with external events (world events, local conditions, operator state).

**Features**:
- Event markers with categories (WorldEvent, LocalEnvironment, OperatorState, Technical, GroupCoherence, Custom)
- Configurable time windows for event correlation
- Welch's t-test and Cohen's d for event vs non-event comparison
- ISO 8601 wall-clock timestamps for post-hoc analysis

### Formal Pre-Registration (`consciousness_prereg` module)

Enhanced pre-registration with machine fingerprinting for cryptographic proof-of-intent.

**Features**:
- SHA-256 parameter hash with full experiment configuration
- Machine fingerprint (OS, architecture, hostname, source list, version)
- Verification hash combining parameters + fingerprint + timestamp
- Save/load/verify pre-registration proof files

## Experiment Controls

### Pre-Registration (`--preregister`)

Generates a SHA-256 hash of experiment parameters before the experiment begins. This hash can be recorded publicly to prevent post-hoc parameter selection (p-hacking).

```bash
openentropy consciousness --preregister --trials 100 --mode standard --output session.json
```

### Double-Blind (`--double-blind`)

Randomizes the intention direction mapping so the operator doesn't know whether their "HIGH" instruction actually corresponds to the High or Low condition. Reveals after the experiment.

```bash
openentropy consciousness --double-blind --trials 100 --output blind_session.json
```

### Operator Profiles (`--operator`)

Tracks cross-session statistics per operator. Profiles are stored in `consciousness_profiles/` as JSON files.

```bash
openentropy consciousness --operator alice --trials 100 --output session.json
openentropy consciousness-profile alice
```

## PRNG Control

Every mode automatically includes a deterministic xorshift64 PRNG (`prng_control`) as a negative control source.

**Rationale**: A PRNG produces deterministic output — no physical process to influence. If the consciousness experiment shows a "significant" effect on the PRNG control:
- The effect is a **statistical artifact** (multiple comparisons, methodological issue)
- NOT evidence of anomalous influence on hardware

**Interpreting PRNG results**:
- PRNG control shows no effect + hardware sources show effect → Genuine hardware anomaly candidate
- PRNG control shows effect → Statistical artifact — increase trial count or check methodology
- All sources show no effect → Null result (expected for most single sessions)

## Statistical Methods

### Stouffer's Z (meta-analysis)
Combines independent Z-scores: `Z_combined = sum(Z_i) / sqrt(k)`

### Cochran's Q (heterogeneity)
Tests whether effect sizes are homogeneous: `Q = sum(w_i * (e_i - e_weighted)^2)` with chi-squared(k-1) distribution.

### I-squared
Percentage of variability due to heterogeneity: `I^2 = max(0, (Q - df) / Q * 100)`

### Benjamini-Hochberg FDR
Controls false discovery rate when testing multiple hypotheses. Sorts p-values and finds the largest rank k where `p_(k) <= k/m * alpha`.

### Fisher Z-transform
Converts Pearson r to normally-distributed Z: `Z = arctanh(r)`. Enables comparison of correlations across conditions with different sample sizes.

### Welch's t-test
Two-sample test for means with unequal variances. Used for structure mode comparisons.

## Replication Protocol

For a rigorous consciousness-RNG study:

1. **Pre-register** your experiment: mode, trial count, alpha level, and hypothesis direction
2. **Run at least 20 sessions** to accumulate statistical power (PEAR Lab ran thousands)
3. **Use `--output` to save JSON** results from every session
4. **Compare across modes**: A genuine effect should show in standard mode (mean shift) AND potentially in structure/coherence modes
5. **Check PRNG control** in every session — it should show null results
6. **Use Bonferroni or BH correction** when analyzing across sources
7. **Report all results**, including null sessions — publication bias distorts meta-analysis

### Recommended session parameters

| Parameter | Quick test | Standard | Deep |
|-----------|-----------|----------|------|
| `--trials` | 10 | 50 | 200 |
| `--bits` | 200 | 200 | 200 |
| `--interval` | 100 | 1000 | 1000 |
| `--epochs` | 2 | 5 | 10 |
| `--epoch-duration` | 5 | 30 | 60 |
| Duration | ~10s | ~3min | ~10min |

## JSON Output Format

All modes produce JSON output via `--output results.json`. The top-level structure is a tagged enum:

```json
{
  "Standard": { ... ExperimentResult ... }
}
```
or
```json
{
  "Spectroscopy": { ... SpectroscopyResult ... }
}
```
etc.

This enables automated aggregation of multi-session results for meta-analysis.

## Novel Analysis Framework (v4)

OpenEntropy v4 introduces 8 novel analytical capabilities never before combined in a consciousness-RNG platform, plus a fused doubly-robust sequential test. These are accessible via `--evalue`, `--deep-analysis`, and `--mode retrocausal`.

### E-Values / Anytime-Valid Inference (`--evalue`)

Replaces traditional p-values with e-values (test martingales) that remain valid under optional stopping — solving the "peeking problem" that plagues all consciousness-RNG research.

**The problem**: In standard hypothesis testing, experimenters who stop when results look promising inflate false-positive rates. With 40+ sources and real-time feedback, the temptation to stop early is strong. P-values become invalid under such "optional stopping."

**The solution**: E-values. Under H0 (Binomial(n, 0.5)), the likelihood ratio for H1 (Binomial(n, 0.5+delta)) is an e-value with E[e] = 1. The running product (wealth process) is a test martingale. Ville's inequality guarantees: P(max W_t >= 1/alpha) <= alpha **for any stopping rule**.

**Evidence levels** (following Vovk's calibration):
- e < 1: no evidence
- 1-3: anecdotal
- 3-10: moderate
- 10-30: strong
- 30-100: very strong
- 100+: decisive

**Usage**:
```bash
# Add e-value reporting to any standard experiment
openentropy consciousness --quick --evalue
openentropy consciousness --trials 100 --evalue --output results.json
```

### Persistent Homology (Topological Data Analysis)

Detects geometric structure in RNG bit streams via delay-coordinate embedding and Vietoris-Rips persistent homology. Part of `--deep-analysis`.

**How it works**:
1. Byte data is embedded into R^d via Takens delay embedding
2. The point cloud is analyzed for topological features: H0 (connected components, via Union-Find) and H1 (loops, via boundary matrix reduction over Z/2Z)
3. Features are summarized as persistence diagrams
4. Baseline vs intention diagrams are compared via Wasserstein distance

**Key insight**: Truly random data produces a featureless point cloud with no significant persistent features. Any topological structure injected by consciousness would appear as long-lived barcodes.

**Metrics**: persistence entropy, total persistence (L1), Wasserstein distance (H0), Betti curve divergence.

**Optimization**: Uses max-min landmark subsampling (farthest-point sampling) for O(n^3) computational feasibility with better coverage than uniform stepping.

### Recurrence Quantification Analysis (RQA)

Recurrence plots detect when a dynamical system revisits previously visited states. Part of `--deep-analysis`.

**Key insight**: For truly random data, determinism = 0 and laminarity = 0 **with mathematical certainty**. Any non-zero determinism during intention epochs is definitive evidence of injected structure — stronger than any p-value threshold.

**Metrics**:
- **Recurrence rate**: fraction of recurrent points (excluding diagonal)
- **Determinism**: fraction of recurrence points forming diagonal lines (DET = 0 for random)
- **Laminarity**: fraction forming vertical lines
- **Trapping time**: average vertical line length
- **Longest diagonal**: Lmax
- **Diagonal entropy**: Shannon entropy of diagonal line length distribution

**Parameters**: embedding dim=3, delay=1, threshold=30.0 (Euclidean in 3D byte space).

### Ordinal Pattern Analysis (Bandt-Pompe)

Permutation entropy and forbidden pattern detection. Part of `--deep-analysis`.

**Key insight**: For a truly random process of sufficient length, all L! ordinal patterns of length L appear with equal probability. **Forbidden patterns** (count = 0) are impossible under randomness and represent definitive non-randomness evidence. This is stronger than any p-value.

**Metrics**:
- **Permutation entropy (PE)**: 0 = deterministic, 1 = maximally random
- **Weighted permutation entropy (WPE)**: amplitude-weighted variant
- **Forbidden patterns**: patterns never observed (definitive non-randomness if data is sufficient)
- **Chi-squared test**: compares pattern distributions between baseline and intention

**Based on**: Bandt & Pompe (2002) "Permutation entropy: a natural complexity measure for time series."

### Transfer Entropy Between Sources

Measures directional information flow between physically independent entropy sources. Part of `--deep-analysis`.

**Key insight**: If consciousness creates coupling between, say, an IMU sensor and NVMe thermal noise (sources with zero physical coupling), the transfer entropy will increase. Under the null, TE between uncoupled sources is ~0. This captures nonlinear directional dependencies that Pearson correlation misses.

**Formula**: TE(X → Y) = H(Y_future | Y_past) - H(Y_future | Y_past, X_past)

**Estimators**:
- **Histogram-based**: 3D joint entropy with proper multivariate binning
- **KSG k-NN**: Kraskov-Stogbauer-Grassberger nearest-neighbor estimator (avoids binning artifacts)

**Based on**: Schreiber (2000) "Measuring information transfer."

### Conformal Prediction (Distribution-Free Anomaly Detection)

Distribution-free anomaly detection with exact finite-sample coverage guarantees. Part of `--deep-analysis`.

**Key insight**: Unlike Mahalanobis distance (which assumes Gaussian), conformal prediction makes **zero distributional assumptions** — only exchangeability of baseline data. The guarantee is exact: P(false positive) <= alpha regardless of distribution.

**Algorithm**:
1. Compute k-NN nonconformity scores for baseline points (leave-one-out)
2. For each intention epoch, compute its nonconformity score
3. Conformal p-value = proportion of calibration scores >= new score
4. Sequential monitoring via conformal martingale (power martingale, epsilon=0.5)

**Rejection**: Ville's inequality — reject if max martingale >= 1/alpha.

**Based on**: Vovk, Gammerman & Shafer (2005) "Algorithmic Learning in a Random World."

### DAT vs Force Model Testing

The first modern platform to explicitly test Decision Augmentation Theory against the Force model. Part of `--deep-analysis`.

**Force model**: Consciousness directly shifts the RNG mean. Predicts: mean shift without excess kurtosis.

**Decision Augmentation Theory (DAT)**: Consciousness doesn't influence the RNG, but the operator unconsciously selects *when* to start and stop the experiment (precognitive selection). Predicts: excess kurtosis (fat tails), temporal clustering of successful trials at experiment start, no true mean shift.

**Analysis**:
- Log-likelihood ratio between models with BIC comparison
- Distributional diagnostics: excess kurtosis, skewness, tail ratio
- Temporal clustering test: whether successes cluster in the first quarter

**Based on**: May, Utts & Spottiswoode (1995) showed DAT fits PEAR data better by 8.6 sigma.

### Retrocausal Protocol (`--mode retrocausal`)

The strongest possible experimental design against conventional artifacts.

**Protocol**:
1. Collect N trials of random bytes — operator does nothing, no intention
2. After ALL data is collected, assign random High/Low directions using xorshift64 PRNG
3. Score each trial as if the assigned direction was the operator's intention
4. Under the null, this is pure chance — any significant result suggests retrocausal influence

**Why it matters**: This eliminates ALL possible real-time influence mechanisms. No electromagnetic interference, no temperature effects, no timing artifacts. If results are significant, the only explanations are: retrocausal influence, or the PRNG direction assignment is correlated with the entropy source (impossible for independent xorshift64).

**Usage**:
```bash
openentropy consciousness --mode retrocausal --quick --interval 50
openentropy consciousness --mode retrocausal --trials 100 --output retro.json
```

**Based on**: Schmidt (1976) "PK effect on pre-recorded targets" and Bem (2011) "Feeling the future."

### Conformal + E-Value Fusion (Doubly-Robust Sequential Monitoring)

Combines conformal martingales (distribution-free) with e-value wealth processes (parametric) for doubly-robust sequential monitoring. Part of `--deep-analysis`.

**How it works**:
1. Two independent channels run simultaneously
2. Channel 1: Conformal martingale detects **any** distributional shift
3. Channel 2: E-value wealth process detects **mean shift** specifically
4. Reject if **EITHER** channel crosses its threshold
5. Bonferroni correction: each channel gets alpha/2

**Theoretical guarantee**: P(false positive) <= alpha by Bonferroni + Ville's inequality, valid under optional stopping for both tests simultaneously.

**Why fusion matters**: Conformal prediction catches subtle distributional anomalies that mean-shift tests miss. E-values catch directional effects that conformal prediction might not flag. Together, they have broader sensitivity than either alone.

### Using the Full Analysis Suite

```bash
# E-values only (fast, adds to standard output)
openentropy consciousness --quick --evalue

# Deep analysis (all 7 novel frameworks on collected data)
openentropy consciousness --quick --deep-analysis

# Both e-values and deep analysis
openentropy consciousness --quick --evalue --deep-analysis

# Retrocausal protocol (separate experiment mode)
openentropy consciousness --mode retrocausal --quick

# Full production experiment with everything
openentropy consciousness --trials 100 --evalue --deep-analysis --preregister --operator alice --output full_session.json

# Deep analysis with 100 surrogate permutation tests + BH FDR correction
openentropy consciousness --quick --deep-analysis --surrogate 100

# Publication-quality: 200 surrogates, pre-registered, double-blind
openentropy consciousness --trials 100 --deep-analysis --surrogate 200 --preregister --double-blind --operator alice
```

### Statistical Hardening (v5)

#### Surrogate/Permutation Testing (`--surrogate N`)

Every deep analysis statistic (ordinal chi-squared, RQA determinism difference, topology
Wasserstein distance, transfer entropy) can be tested against a null distribution generated
by N random shuffles of the data. This provides:

- **Empirical p-values**: Fraction of surrogates with more extreme statistics than observed
- **Effect sizes**: Cohen's d against the null distribution
- **BH FDR correction**: Benjamini-Hochberg q-values across all tests to control false
  discovery rate for multiple testing

The final "Section 8: Surrogate Significance Summary" table shows all test statistics with
their raw p-values, FDR-corrected q-values, z-scores, and effect sizes. Tests marked with
`*` survived FDR correction at alpha=0.05.

#### Higher-Order Transfer Entropy

Multi-lag past embeddings (`transfer_entropy_higher_order`, `transfer_entropy_knn_higher_order`)
condition on (Y_{t-1}, ..., Y_{t-order}) instead of just Y_{t-1}. This captures coupling
that only manifests at longer timescales (e.g., a source that predicts another source
2 steps ahead but not 1 step ahead).

#### Deep Analysis Pre-Registration

When `--preregister` is combined with `--deep-analysis`, the exact analysis configuration
(ordinal order, RQA dimensions, TE bins, conformal alpha, etc.) is hashed into the
pre-registration record. This prevents post-hoc parameter selection across the deep
analysis pipeline.

#### Cross-Session Conformal Calibration (`--calibration-file`)

Conformal prediction calibration sets can be saved, loaded, and merged across sessions.
This allows accumulating baseline data from multiple sessions for more powerful anomaly
detection, without requiring a fresh baseline each time.

```bash
# First session: creates calibration file
openentropy consciousness --quick --deep-analysis --calibration-file cal.json

# Subsequent sessions: loads existing calibration, merges with new baseline, saves updated
openentropy consciousness --quick --deep-analysis --calibration-file cal.json
```

#### Configurable Transfer Entropy Order (`--te-order`)

Higher-order transfer entropy embeddings can be configured via `--te-order N`. The default
is 1 (single-lag). Higher orders (2-5) condition on multi-lag past history to capture
slow, multi-timescale coupling between entropy sources.

```bash
openentropy consciousness --quick --deep-analysis --te-order 3
```

#### Adaptive Surrogate Count

The `adaptive_surrogate_test` function starts with a small surrogate count (e.g., 100)
and automatically escalates to a larger count (e.g., 1000) for borderline results
(0.01 < p < 0.10). This saves compute for clear-cut results while providing higher
precision for ambiguous cases.

#### Bootstrap Confidence Intervals

All surrogate test results now include 95% bootstrap confidence intervals for Cohen's d
effect sizes (when n_surrogates >= 50). The BH FDR summary table (section 8) displays
CI bounds alongside point estimates, showing not just *whether* an effect is significant
but *how precise* the estimate is.

#### TUI Forest Plot View

The consciousness TUI dashboard (`--tui`) now includes a 5th view (Tab to cycle): a
publication-grade forest plot showing per-source mean Z-scores with +/- 1 SE confidence
bars. Sources are sorted by absolute effect size, the PRNG control is labeled `[C]`,
and a pooled estimate row appears at the bottom. This provides real-time meta-analytic
visualization during experiments.

## Limitations and Caveats

1. **Single sessions are underpowered**: The PEAR Lab effect size (~0.0003) requires thousands of trials to detect reliably. A single 50-trial session has very low power. Use `consciousness-meta` to accumulate evidence.
2. **Multiple comparisons**: With 40+ sources, expect ~2 false positives at alpha=0.05 by chance alone. Always apply FDR correction and check PRNG control.
3. **Blinding**: Use `--double-blind` to randomize direction mapping. Single-operator double-blind is supported.
4. **Hardware variability**: Source availability and performance vary by machine. Cross-machine replication strengthens claims.
5. **SHA-256 conditioning**: While necessary for unbiased bits, conditioning may attenuate any real physical effect. Raw-byte experiments could be more sensitive but violate the Binomial null hypothesis.
6. **Pre-registration**: Use `--preregister` to generate parameter hashes before experiments to prevent p-hacking.
7. **Learning effects**: Feedback mode can create expectation bias. Combine with standard mode for independent validation.
