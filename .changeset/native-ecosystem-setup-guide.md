---
monochange_app: fix
---

# Document native package release setup

The hosted ecosystem guide now explains how to configure Swift, CocoaPods, and Gradle packages, including versioned files and registry credentials. Follow the native ecosystem sections when adding Apple or Android packages to an existing workspace; Gradle and CocoaPods publishing require `trusted_publishing = false` and their registry credentials.
