# RFC-012: 構造的成果物変換 (Structural Artifact Transformer)

## Purpose
L2 グラフ（概念構造）を、具体的な開発成果物（Rust コード、SQL スキーマ、Mermaid 図等）のテンプレートへ自動変換し、設計から実装への移行を支援する。

## Interfaces

### HybridVM API
- `fn generate_artifacts(&self, format: ArtifactFormat) -> Result<Vec<GeneratedArtifact>, SemanticError>`

### CLI Command
- `design export --format <rust|sql|mermaid> --out <dir>`

## Data Structures

```rust
pub enum ArtifactFormat {
    Rust,
    Sql,
    Mermaid,
}

pub struct GeneratedArtifact {
    pub file_name: String,
    pub content: String,
}
```

## Logic
1. **L2 分析**: 各 L2 コンセプトの `derived_requirements` をスキャン。
2. **テンプレート適用**: 
   - `Performance` や `Memory` 要件を含むコンセプト -> 高効率なデータ構造や非同期処理のスケルトンを生成。
   - `CausalLinks` -> 依存関係（Rust の `use` や SQL の `FOREIGN KEY`）としてマッピング。
3. **メタデータ付与**: 生成された成果物には、元となった L2 コンセプトのハッシュや ID を含め、追跡可能性（Traceability）を確保する。

## Success Criteria
- [ ] 複数の L2 コンセプトから、整合性の取れた Rust コードのスケルトンが生成されること。
- [ ] エッジ（因果関係）がコード上の依存関係として正しく表現されていること。
- [ ] 生成されたファイルが指定ディレクトリに出力されること。
