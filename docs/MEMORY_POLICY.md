# RustBrowse Memory Policy (4 GB RAM Target Tier)

## Tab Lifecycle State Machine
```
Active ──(hide > 30s)──> Warm ──(hide > 90s)──> Sleeping ──(hide > 300s)──> Discarded
  ▲                       │                        │                           │
  └───────────────────────┴────────────────────────┴───────────────────────────┘
                               Focus / Switch Tab
```

### State Behaviors
1. **Active**:
   - `IsVisible = true`
   - `MemoryUsageTargetLevel = Normal`
2. **Warm**:
   - `IsVisible = false`
   - Instant tab switch back (< 16 ms)
3. **Sleeping**:
   - `IsVisible = false`
   - `MemoryUsageTargetLevel = Low`
   - `TrySuspend` invoked
4. **Discarded**:
   - Controller closed and dropped from memory
   - Metadata (`TabMetadata`) kept in memory
   - Upon switch, new Controller created and navigated lazily

### Exemption Rules (Never Auto-Sleep or Discard)
- Tab playing audio/video (`is_audible = true`)
- Tab with active downloads (`is_downloading = true`)
- Pinned tabs (`is_pinned = true`)
- User-marked exempt tabs (`user_protected = true`)

### Pressure Matrix
| Tier | Available RAM | Memory Load | Action |
|---|---|---|---|
| Normal | > 1024 MB | < 85% | 1 Active + max 2 Warm/Sleeping |
| Warning | < 1024 MB | < 85% | 1 Active + 1 Warm, Sleep older tabs |
| Critical | < 600 MB | >= 85% | Discard eligible tabs aggressively |
