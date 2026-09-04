use glam::IVec3;

use crate::impact::event::ImpactEvent;
use crate::impact::volume::AffectedVolume;

/// Hasil pemrosesan kanonikal suatu benturan bersama dengan query volume spasialnya.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessedImpact {
    pub event: ImpactEvent,
    pub volume: AffectedVolume,
}

/// Pipeline pemrosesan benturan deterministik (Deterministic Impact Pipeline).
///
/// Tanggung jawab utama:
/// 1. Menerima kumpulan `ImpactEvent` dari berbagai sumber.
/// 2. Mengurutkan kumpulan event secara kanonikal deterministik tanpa bergantung pada
///    alokasi memori, hash non-deterministik, atau urutan thread.
/// 3. Menghitung query volume terpengaruh (`AffectedVolume`) untuk setiap event.
/// 4. Menyediakan antarmuka kueri spasial tanpa memutasi dunia atau status fisika apa pun.
#[derive(Debug, Clone, Default)]
pub struct DeterministicImpactPipeline {
    events: Vec<ImpactEvent>,
}

impl DeterministicImpactPipeline {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Memasukkan satu event benturan ke dalam pipeline.
    pub fn submit(&mut self, event: ImpactEvent) {
        self.events.push(event);
    }

    /// Memasukkan kumpulan event benturan ke dalam pipeline.
    pub fn submit_batch<I: IntoIterator<Item = ImpactEvent>>(&mut self, iter: I) {
        self.events.extend(iter);
    }

    /// Jumlah event yang tersimpan di dalam buffer pipeline saat ini.
    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Apakah pipeline kosong.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Mengosongkan event di dalam buffer pipeline.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Memproses seluruh event yang telah disubmit:
    /// 1. Mengurutkan secara kanonikal deterministik.
    /// 2. Menghitung volume spasial terpengaruh untuk setiap event.
    ///
    /// CATATAN DETERMINISME:
    /// Urutan output dijamin 100% identik bit demi bit untuk sekumpulan event input
    /// yang sama, bahkan jika urutan input disubmit secara acak/berbeda.
    pub fn process(&self) -> Vec<ProcessedImpact> {
        let mut sorted_events = self.events.clone();
        sorted_events.sort();

        sorted_events
            .into_iter()
            .map(|event| {
                let volume = AffectedVolume::from_event(&event);
                ProcessedImpact { event, volume }
            })
            .collect()
    }

    /// Memproses event dengan menyaring duplikasi ID (deduplikasi berdasarkan ImpactId),
    /// mempertahankan hanya entri pertama berdasarkan urutan kanonikal.
    pub fn process_and_deduplicate(&self) -> Vec<ProcessedImpact> {
        let mut sorted_events = self.events.clone();
        sorted_events.sort();
        sorted_events.dedup_by_key(|e| e.id);

        sorted_events
            .into_iter()
            .map(|event| {
                let volume = AffectedVolume::from_event(&event);
                ProcessedImpact { event, volume }
            })
            .collect()
    }

    /// Mengumpulkan seluruh koordinat chunk unik yang bersinggungan dengan
    /// benturan-benturan di dalam pipeline, terurut secara kanonikal deterministik (Y -> Z -> X).
    pub fn query_affected_chunks(&self) -> Vec<IVec3> {
        let processed = self.process();
        let mut chunks = Vec::new();

        for item in processed {
            for chunk in item.volume.iter_chunks() {
                chunks.push(chunk);
            }
        }

        chunks.sort_by_key(|a| (a.y, a.z, a.x));
        chunks.dedup();
        chunks
    }
}
