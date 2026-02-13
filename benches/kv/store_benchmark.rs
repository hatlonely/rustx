use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use num_cpus;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rustx::kv::store::{
    DashMapStore, DashMapStoreConfig, RwLockHashMapStore, RwLockHashMapStoreConfig,
    SetOptions, SyncStore, UnsafeHashMapStore, UnsafeHashMapStoreConfig,
};
use std::sync::Arc;

const NUM_ITEMS: usize = 1_000_000;

// ========== 辅助函数 ==========

fn generate_key(i: usize) -> String {
    format!("key_{:010}", i)
}

fn generate_value(i: usize) -> String {
    format!("value_{:010}", i)
}

// ========== 1. 单线程顺序写入 ==========

fn benchmark_sequential_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_write");

    for store_type in ["DashMapStore", "RwLockHashMapStore", "UnsafeHashMapStore"] {
        group.bench_with_input(BenchmarkId::from_parameter(store_type), &store_type, |b, store_type| {
            b.iter(|| {
                let store: Arc<dyn SyncStore<String, String>> = match *store_type {
                    "DashMapStore" => Arc::new(DashMapStore::new(DashMapStoreConfig::default())),
                    "RwLockHashMapStore" => {
                        Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
                    }
                    "UnsafeHashMapStore" => {
                        Arc::new(UnsafeHashMapStore::new(UnsafeHashMapStoreConfig::default()))
                    }
                    _ => return,
                };

                for i in 0..NUM_ITEMS {
                    let key = generate_key(i);
                    let value = generate_value(i);
                    black_box(store.set_sync(&key, &value, &SetOptions::new()).unwrap());
                }
            })
        });
    }

    group.finish();
}

// ========== 2. 单线程顺序读取 ==========

fn benchmark_sequential_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_read");

    for store_type in ["DashMapStore", "RwLockHashMapStore", "UnsafeHashMapStore"] {
        // 准备数据
        let store: Arc<dyn SyncStore<String, String>> = match store_type {
            "DashMapStore" => Arc::new(DashMapStore::new(DashMapStoreConfig::default())),
            "RwLockHashMapStore" => {
                Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
            }
            "UnsafeHashMapStore" => {
                Arc::new(UnsafeHashMapStore::new(UnsafeHashMapStoreConfig::default()))
            }
            _ => continue,
        };

        for i in 0..NUM_ITEMS {
            let key = generate_key(i);
            let value = generate_value(i);
            store.set_sync(&key, &value, &SetOptions::new()).unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(store_type), &store_type, |b, _| {
            b.iter(|| {
                for i in 0..NUM_ITEMS {
                    let key = generate_key(i);
                    black_box(store.get_sync(&key).unwrap());
                }
            })
        });
    }

    group.finish();
}

// ========== 3. 多线程并发读 ==========

fn benchmark_concurrent_read(c: &mut Criterion) {
    let num_threads = num_cpus::get();
    let mut group = c.benchmark_group("concurrent_read");

    for store_type in ["DashMapStore", "RwLockHashMapStore", "UnsafeHashMapStore"] {
        // 准备数据 - 每个线程都需要能够读取到所有数据
        let store: Arc<dyn SyncStore<String, String>> = match store_type {
            "DashMapStore" => Arc::new(DashMapStore::new(DashMapStoreConfig::default())),
            "RwLockHashMapStore" => {
                Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
            }
            "UnsafeHashMapStore" => {
                Arc::new(UnsafeHashMapStore::new(UnsafeHashMapStoreConfig::default()))
            }
            _ => continue,
        };

        // 单线程写入数据
        for i in 0..NUM_ITEMS {
            let key = generate_key(i);
            let value = generate_value(i);
            store.set_sync(&key, &value, &SetOptions::new()).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new(store_type, num_threads),
            &(store_type, num_threads),
            |b, (_, num_threads)| {
                b.iter(|| {
                    let reads_per_thread = NUM_ITEMS / num_threads;

                    (0..*num_threads).into_par_iter().for_each(|thread_id| {
                        let start = thread_id * reads_per_thread;
                        let end = if thread_id == num_threads - 1 {
                            NUM_ITEMS
                        } else {
                            start + reads_per_thread
                        };

                        for i in start..end {
                            let key = generate_key(i);
                            black_box(store.get_sync(&key).unwrap());
                        }
                    });
                })
            },
        );
    }

    group.finish();
}

// ========== 4. 多线程并发写（不含 UnsafeHashMapStore）==========

fn benchmark_concurrent_write(c: &mut Criterion) {
    let num_threads = num_cpus::get();
    let mut group = c.benchmark_group("concurrent_write");

    for store_type in ["DashMapStore", "RwLockHashMapStore"] {
        group.bench_with_input(
            BenchmarkId::new(store_type, num_threads),
            &(store_type, num_threads),
            |b, (store_type, num_threads)| {
                b.iter(|| {
                    let store: Arc<dyn SyncStore<String, String>> = if **store_type == *"DashMapStore" {
                        Arc::new(DashMapStore::new(DashMapStoreConfig::default()))
                    } else {
                        Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
                    };

                    let writes_per_thread = NUM_ITEMS / num_threads;

                    (0..*num_threads).into_par_iter().for_each(|thread_id| {
                        let start = thread_id * writes_per_thread;
                        let end = if thread_id == num_threads - 1 {
                            NUM_ITEMS
                        } else {
                            start + writes_per_thread
                        };

                        for i in start..end {
                            let key = generate_key(i);
                            let value = generate_value(i);
                            black_box(store.set_sync(&key, &value, &SetOptions::new()).unwrap());
                        }
                    });
                })
            },
        );
    }

    group.finish();
}

// ========== 5. 混合读写（70% 读 + 30% 写，不含 UnsafeHashMapStore）==========

fn benchmark_mixed_read_write(c: &mut Criterion) {
    let num_threads = num_cpus::get();
    let mut group = c.benchmark_group("mixed_read_write");

    // 先写入部分数据供读取
    let initial_data = NUM_ITEMS / 2;

    for store_type in ["DashMapStore", "RwLockHashMapStore"] {
        group.bench_with_input(
            BenchmarkId::new(store_type, num_threads),
            &(store_type, num_threads),
            |b, (store_type, num_threads)| {
                b.iter(|| {
                    let store: Arc<dyn SyncStore<String, String>> = if **store_type == *"DashMapStore" {
                        Arc::new(DashMapStore::new(DashMapStoreConfig::default()))
                    } else {
                        Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
                    };

                    // 预先写入部分数据
                    for i in 0..initial_data {
                        let key = generate_key(i);
                        let value = generate_value(i);
                        store.set_sync(&key, &value, &SetOptions::new()).unwrap();
                    }

                    let ops_per_thread = NUM_ITEMS / num_threads;

                    (0..*num_threads).into_par_iter().for_each(|thread_id| {
                        let base_idx = thread_id * ops_per_thread;

                        for op in 0..ops_per_thread {
                            let idx = base_idx + op;

                            // 70% 读，30% 写
                            if op % 10 < 7 {
                                // 读：读取已有的数据
                                let read_idx = idx % initial_data;
                                let key = generate_key(read_idx);
                                black_box(store.get_sync(&key).ok());
                            } else {
                                // 写：写入新数据
                                let key = generate_key(initial_data + idx);
                                let value = generate_value(initial_data + idx);
                                black_box(store.set_sync(&key, &value, &SetOptions::new()).ok());
                            }
                        }
                    });
                })
            },
        );
    }

    group.finish();
}

// ========== 6. 批量写入 ==========

fn benchmark_batch_write(c: &mut Criterion) {
    let batch_size = 1000;
    let num_batches = NUM_ITEMS / batch_size;

    let mut group = c.benchmark_group("batch_write");

    for store_type in ["DashMapStore", "RwLockHashMapStore", "UnsafeHashMapStore"] {
        group.bench_with_input(
            BenchmarkId::new(store_type, batch_size),
            &store_type,
            |b, store_type| {
                b.iter(|| {
                    let store: Arc<dyn SyncStore<String, String>> = if **store_type == *"DashMapStore" {
                        Arc::new(DashMapStore::new(DashMapStoreConfig::default()))
                    } else if **store_type == *"RwLockHashMapStore" {
                        Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
                    } else {
                        Arc::new(UnsafeHashMapStore::new(UnsafeHashMapStoreConfig::default()))
                    };

                    for batch in 0..num_batches {
                        let start = batch * batch_size;
                        let keys: Vec<String> =
                            (start..start + batch_size).map(|i| generate_key(i)).collect();
                        let values: Vec<String> =
                            (start..start + batch_size).map(|i| generate_value(i)).collect();

                        black_box(
                            store
                                .batch_set_sync(&keys, &values, &SetOptions::new())
                                .unwrap(),
                        );
                    }
                })
            },
        );
    }

    group.finish();
}

// ========== 7. 批量读取 ==========

fn benchmark_batch_read(c: &mut Criterion) {
    let batch_size = 1000;
    let num_batches = NUM_ITEMS / batch_size;

    let mut group = c.benchmark_group("batch_read");

    for store_type in ["DashMapStore", "RwLockHashMapStore", "UnsafeHashMapStore"] {
        // 准备数据
        let store: Arc<dyn SyncStore<String, String>> = match store_type {
            "DashMapStore" => Arc::new(DashMapStore::new(DashMapStoreConfig::default())),
            "RwLockHashMapStore" => {
                Arc::new(RwLockHashMapStore::new(RwLockHashMapStoreConfig::default()))
            }
            "UnsafeHashMapStore" => {
                Arc::new(UnsafeHashMapStore::new(UnsafeHashMapStoreConfig::default()))
            }
            _ => continue,
        };

        for i in 0..NUM_ITEMS {
            let key = generate_key(i);
            let value = generate_value(i);
            store.set_sync(&key, &value, &SetOptions::new()).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new(store_type, batch_size),
            &store_type,
            |b, _| {
                b.iter(|| {
                    for batch in 0..num_batches {
                        let start = batch * batch_size;
                        let keys: Vec<String> =
                            (start..start + batch_size).map(|i| generate_key(i)).collect();

                        black_box(store.batch_get_sync(&keys).unwrap());
                    }
                })
            },
        );
    }

    group.finish();
}

// ========== 主函数 ==========

criterion_group!(
    benches,
    benchmark_sequential_write,
    benchmark_sequential_read,
    benchmark_concurrent_read,
    benchmark_concurrent_write,
    benchmark_mixed_read_write,
    benchmark_batch_write,
    benchmark_batch_read
);
criterion_main!(benches);
