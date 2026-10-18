# NVIDIA Nemotron 3 Diarization

Model supplied by NVIDIA under OpenMDW-1.1; the complete agreement is included alongside this notice.

- Model: `nvidia/Nemotron-3-Diarization`
- Source revision: `f667ed73aee57d40cc39428eb768b4fd87a0a29e`
- Official Q8_0 artifact SHA-256: `08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1`
- Source: https://huggingface.co/nvidia/Nemotron-3-Diarization
- License source: https://openmdw.ai/license/1-1/
- Runtime: NVIDIA NeMo-Speech.cpp 0.2.0, https://github.com/NVIDIA/NeMo-Speech.cpp/releases/tag/v0.2.0

The runtime archive retains its own included notices and licenses under `/opt/nemo/share`. The model weights are not modified. Runtime returns anonymous speaker channels and estimated timestamps from bounded recorded audio; it does not identify people.
