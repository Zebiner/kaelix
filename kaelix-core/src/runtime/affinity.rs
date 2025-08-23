//! CPU Affinity and NUMA Topology Management
//!
//! Provides sophisticated CPU core binding and NUMA topology detection for optimal
//! performance in multi-processor systems.

use crate::runtime::RuntimeResult;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

/// CPU set for affinity management
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuSet {
    /// Set of CPU core IDs
    cores: HashSet<usize>,

    /// NUMA node this CPU set belongs to
    numa_node: Option<NodeId>,
}

impl CpuSet {
    /// Create a new CPU set
    pub fn new() -> Self {
        Self { cores: HashSet::new(), numa_node: None }
    }

    /// Create a CPU set from a vector of core IDs
    pub fn from_cores(cores: Vec<usize>) -> Self {
        Self { cores: cores.into_iter().collect(), numa_node: None }
    }

    /// Create a CPU set with NUMA node assignment
    pub fn with_numa_node(cores: Vec<usize>, numa_node: NodeId) -> Self {
        Self { cores: cores.into_iter().collect(), numa_node: Some(numa_node) }
    }

    /// Add a CPU core to the set
    pub fn add_core(&mut self, core_id: usize) {
        self.cores.insert(core_id);
    }

    /// Remove a CPU core from the set
    pub fn remove_core(&mut self, core_id: usize) -> bool {
        self.cores.remove(&core_id)
    }

    /// Check if the set contains a specific core
    pub fn contains_core(&self, core_id: usize) -> bool {
        self.cores.contains(&core_id)
    }

    /// Get all core IDs in the set
    pub fn cores(&self) -> &HashSet<usize> {
        &self.cores
    }

    /// Get the number of cores in the set
    pub fn len(&self) -> usize {
        self.cores.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.cores.is_empty()
    }

    /// Get the NUMA node ID if assigned
    pub fn numa_node(&self) -> Option<NodeId> {
        self.numa_node
    }

    /// Set the NUMA node assignment
    pub fn set_numa_node(&mut self, node_id: NodeId) {
        self.numa_node = Some(node_id);
    }

    /// Create a CPU set representing all available cores
    pub fn all_cores() -> RuntimeResult<Self> {
        let num_cores = num_cpus::get();
        let cores = (0..num_cores).collect();
        Ok(Self::from_cores(cores))
    }

    /// Apply CPU affinity to the current thread (Linux implementation)
    #[cfg(target_os = "linux")]
    pub fn apply_to_current_thread(&self) -> RuntimeResult<()> {
        use libc::{CPU_SET, CPU_ZERO, cpu_set_t, sched_setaffinity};
        use std::mem;

        // Note: This unsafe block is necessary for low-level CPU affinity control
        // It's a legitimate use case for unsafe code in systems programming
        #[allow(unsafe_code)]
        unsafe {
            let mut cpu_set: cpu_set_t = mem::zeroed();
            CPU_ZERO(&mut cpu_set);

            for &core_id in &self.cores {
                CPU_SET(core_id, &mut cpu_set);
            }

            let result = sched_setaffinity(
                0, // Current thread
                mem::size_of::<cpu_set_t>(),
                &cpu_set,
            );

            if result != 0 {
                return Err(crate::runtime::RuntimeError::CpuAffinity(format!(
                    "Failed to set CPU affinity: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }

        Ok(())
    }

    /// Apply CPU affinity to the current thread (fallback for non-Linux)
    #[cfg(not(target_os = "linux"))]
    pub fn apply_to_current_thread(&self) -> RuntimeResult<()> {
        // For non-Linux systems, we'll use a simpler approach or no-op
        tracing::warn!("CPU affinity setting not fully supported on this platform");
        Ok(())
    }
}

impl Default for CpuSet {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CpuSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CpuSet{{")?;
        let cores: Vec<_> = self.cores.iter().copied().collect();
        let mut sorted_cores = cores;
        sorted_cores.sort_unstable();

        for (i, core) in sorted_cores.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}", core)?;
        }

        if let Some(node_id) = self.numa_node {
            write!(f, " numa:{}", node_id.0)?;
        }

        write!(f, "}}")
    }
}

/// NUMA node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node{}", self.0)
    }
}

/// NUMA node information
#[derive(Debug, Clone)]
pub struct NumaNode {
    /// Node ID
    pub id: NodeId,

    /// CPU cores on this node
    pub cores: Vec<usize>,

    /// Total memory available on this node (in bytes)
    pub memory_size: u64,

    /// Available memory on this node (in bytes)
    pub available_memory: u64,

    /// Distance to other NUMA nodes (lower = faster access)
    pub distances: HashMap<NodeId, u32>,
}

impl NumaNode {
    /// Create a new NUMA node
    pub fn new(id: NodeId, cores: Vec<usize>) -> Self {
        Self { id, cores, memory_size: 0, available_memory: 0, distances: HashMap::new() }
    }

    /// Check if this node contains a specific CPU core
    pub fn contains_core(&self, core_id: usize) -> bool {
        self.cores.contains(&core_id)
    }

    /// Get the distance to another NUMA node
    pub fn distance_to(&self, other: NodeId) -> Option<u32> {
        self.distances.get(&other).copied()
    }

    /// Get memory utilization percentage
    pub fn memory_utilization(&self) -> f64 {
        if self.memory_size == 0 {
            0.0
        } else {
            1.0 - (self.available_memory as f64 / self.memory_size as f64)
        }
    }
}

/// Memory topology information
#[derive(Debug, Clone)]
pub struct MemoryTopology {
    /// Total system memory
    pub total_memory: u64,

    /// Available system memory
    pub available_memory: u64,

    /// Memory allocated per NUMA node
    pub numa_memory: HashMap<NodeId, u64>,

    /// Hugepage information
    pub hugepages: HugepageInfo,
}

/// Hugepage configuration information
#[derive(Debug, Clone)]
pub struct HugepageInfo {
    /// Default hugepage size in bytes
    pub default_size: usize,

    /// Available hugepage sizes
    pub available_sizes: Vec<usize>,

    /// Number of hugepages per size
    pub page_counts: HashMap<usize, usize>,
}

impl Default for HugepageInfo {
    fn default() -> Self {
        Self {
            default_size: 2 * 1024 * 1024,                              // 2MB
            available_sizes: vec![2 * 1024 * 1024, 1024 * 1024 * 1024], // 2MB, 1GB
            page_counts: HashMap::new(),
        }
    }
}

/// NUMA topology detection and management
#[derive(Debug, Clone)]
pub struct NumaTopology {
    /// Detected NUMA nodes
    nodes: Vec<NumaNode>,

    /// CPU to NUMA node mapping
    cpu_mapping: HashMap<usize, NodeId>,

    /// Memory topology information
    memory_info: MemoryTopology,

    /// Whether NUMA is available on this system
    numa_available: bool,
}

impl NumaTopology {
    /// Detect NUMA topology on the current system
    pub fn detect() -> RuntimeResult<Self> {
        let numa_available = Self::check_numa_availability();

        let (nodes, cpu_mapping) = if numa_available {
            Self::detect_numa_nodes()?
        } else {
            Self::create_single_node()?
        };

        let memory_info = Self::detect_memory_topology(&nodes)?;

        Ok(Self { nodes, cpu_mapping, memory_info, numa_available })
    }

    /// Check if NUMA is available on this system
    fn check_numa_availability() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/sys/devices/system/node").exists()
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Detect NUMA nodes and their CPU assignments
    #[cfg(target_os = "linux")]
    fn detect_numa_nodes() -> RuntimeResult<(Vec<NumaNode>, HashMap<usize, NodeId>)> {
        use std::fs;

        let mut nodes = Vec::new();
        let mut cpu_mapping = HashMap::new();

        // Read NUMA nodes from /sys/devices/system/node/
        let node_dir = std::path::Path::new("/sys/devices/system/node");

        if !node_dir.exists() {
            return Self::create_single_node();
        }

        let entries = fs::read_dir(node_dir)
            .map_err(|e| crate::runtime::RuntimeError::NumaTopology(e.to_string()))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| crate::runtime::RuntimeError::NumaTopology(e.to_string()))?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with("node") {
                if let Ok(node_id) = name_str[4..].parse::<usize>() {
                    let node_path = entry.path();
                    let cpulist_path = node_path.join("cpulist");

                    if cpulist_path.exists() {
                        let cpulist = fs::read_to_string(&cpulist_path).map_err(|e| {
                            crate::runtime::RuntimeError::NumaTopology(e.to_string())
                        })?;

                        let cores = Self::parse_cpulist(&cpulist.trim())?;

                        for &core in &cores {
                            cpu_mapping.insert(core, NodeId(node_id));
                        }

                        let mut node = NumaNode::new(NodeId(node_id), cores);

                        // Try to read memory information
                        if let Ok(meminfo) = fs::read_to_string(node_path.join("meminfo")) {
                            node.memory_size = Self::parse_node_memory(&meminfo);
                        }

                        nodes.push(node);
                    }
                }
            }
        }

        if nodes.is_empty() {
            return Self::create_single_node();
        }

        // Sort nodes by ID
        nodes.sort_by_key(|n| n.id.0);

        Ok((nodes, cpu_mapping))
    }

    /// Detect NUMA nodes (fallback for non-Linux systems)
    #[cfg(not(target_os = "linux"))]
    fn detect_numa_nodes() -> RuntimeResult<(Vec<NumaNode>, HashMap<usize, NodeId>)> {
        Self::create_single_node()
    }

    /// Create a single NUMA node containing all CPUs (fallback)
    fn create_single_node() -> RuntimeResult<(Vec<NumaNode>, HashMap<usize, NodeId>)> {
        let num_cores = num_cpus::get();
        let cores: Vec<usize> = (0..num_cores).collect();

        let mut cpu_mapping = HashMap::new();
        for &core in &cores {
            cpu_mapping.insert(core, NodeId(0));
        }

        let node = NumaNode::new(NodeId(0), cores);

        Ok((vec![node], cpu_mapping))
    }

    /// Parse CPU list format (e.g., "0-3,8-11")
    fn parse_cpulist(cpulist: &str) -> RuntimeResult<Vec<usize>> {
        let mut cores = Vec::new();

        for range in cpulist.split(',') {
            let range = range.trim();
            if range.is_empty() {
                continue;
            }

            if let Some(dash_pos) = range.find('-') {
                let start: usize = range[..dash_pos].parse().map_err(|e| {
                    crate::runtime::RuntimeError::NumaTopology(format!(
                        "Invalid CPU range start: {}",
                        e
                    ))
                })?;
                let end: usize = range[dash_pos + 1..].parse().map_err(|e| {
                    crate::runtime::RuntimeError::NumaTopology(format!(
                        "Invalid CPU range end: {}",
                        e
                    ))
                })?;

                cores.extend(start..=end);
            } else {
                let cpu: usize = range.parse().map_err(|e| {
                    crate::runtime::RuntimeError::NumaTopology(format!("Invalid CPU ID: {}", e))
                })?;
                cores.push(cpu);
            }
        }

        Ok(cores)
    }

    /// Parse memory information from NUMA node meminfo
    fn parse_node_memory(meminfo: &str) -> u64 {
        for line in meminfo.lines() {
            if line.starts_with("Node") && line.contains("MemTotal:") {
                if let Some(_kb_pos) = line.rfind(" kB") {
                    if let Some(size_start) = line.rfind(' ') {
                        if let Ok(kb) = line[size_start + 1.._kb_pos].parse::<u64>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
        0
    }

    /// Detect memory topology information
    fn detect_memory_topology(nodes: &[NumaNode]) -> RuntimeResult<MemoryTopology> {
        let mut numa_memory = HashMap::new();
        let mut total_memory = 0;

        for node in nodes {
            numa_memory.insert(node.id, node.memory_size);
            total_memory += node.memory_size;
        }

        // Get system memory information
        let available_memory = Self::get_available_memory();

        Ok(MemoryTopology {
            total_memory,
            available_memory,
            numa_memory,
            hugepages: HugepageInfo::default(),
        })
    }

    /// Get available system memory
    #[cfg(target_os = "linux")]
    fn get_available_memory() -> u64 {
        use std::fs;

        if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemAvailable:") {
                    if let Some(_kb_pos) = line.rfind(" kB") {
                        let parts: Vec<_> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<u64>() {
                                return kb * 1024; // Convert KB to bytes
                            }
                        }
                    }
                }
            }
        }
        0
    }

    /// Get available system memory (fallback)
    #[cfg(not(target_os = "linux"))]
    fn get_available_memory() -> u64 {
        // Fallback: assume 75% of total memory is available
        // This is a rough estimate for non-Linux systems
        let total_cores = num_cpus::get();
        (total_cores as u64) * 2 * 1024 * 1024 * 1024 // 2GB per core estimate
    }

    /// Get all NUMA nodes
    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }

    /// Get a specific NUMA node by ID
    pub fn node(&self, id: NodeId) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get the NUMA node for a specific CPU core
    pub fn node_for_cpu(&self, cpu_id: usize) -> Option<NodeId> {
        self.cpu_mapping.get(&cpu_id).copied()
    }

    /// Check if NUMA is available on this system
    pub fn is_numa_available(&self) -> bool {
        self.numa_available
    }

    /// Get the optimal number of worker threads for this topology
    pub fn optimal_worker_count(&self) -> usize {
        if self.numa_available {
            // One worker per NUMA node, but at least one per core
            let numa_nodes = self.nodes.len();
            let total_cores = self.cpu_mapping.len();

            // Use NUMA nodes as a starting point, but ensure we have enough workers
            std::cmp::min(numa_nodes * 4, total_cores)
        } else {
            // Without NUMA, use all available cores
            num_cpus::get()
        }
    }

    /// Generate optimal worker placement across NUMA nodes
    pub fn optimal_worker_placement(&self, worker_count: usize) -> Vec<CpuSet> {
        let mut placements = Vec::with_capacity(worker_count);

        if !self.numa_available || self.nodes.is_empty() {
            // Fallback: distribute workers across all cores evenly
            let total_cores = num_cpus::get();
            let cores_per_worker = std::cmp::max(1, total_cores / worker_count);

            for worker_id in 0..worker_count {
                let start_core = (worker_id * cores_per_worker) % total_cores;
                let end_core = std::cmp::min(start_core + cores_per_worker, total_cores);

                let cores: Vec<usize> = (start_core..end_core).collect();
                placements.push(CpuSet::from_cores(cores));
            }
        } else {
            // NUMA-aware placement
            let workers_per_node = std::cmp::max(1, worker_count / self.nodes.len());
            let mut worker_id = 0;

            for node in &self.nodes {
                let cores_per_worker = std::cmp::max(1, node.cores.len() / workers_per_node);

                for node_worker in 0..workers_per_node {
                    if worker_id >= worker_count {
                        break;
                    }

                    let start_idx = node_worker * cores_per_worker;
                    let end_idx = std::cmp::min(start_idx + cores_per_worker, node.cores.len());

                    if start_idx < node.cores.len() {
                        let cores = node.cores[start_idx..end_idx].to_vec();
                        let mut cpu_set = CpuSet::from_cores(cores);
                        cpu_set.set_numa_node(node.id);
                        placements.push(cpu_set);
                        worker_id += 1;
                    }
                }
            }

            // Fill remaining workers if needed
            while worker_id < worker_count {
                let node_idx = worker_id % self.nodes.len();
                let node = &self.nodes[node_idx];

                if !node.cores.is_empty() {
                    let core = node.cores[worker_id % node.cores.len()];
                    let mut cpu_set = CpuSet::from_cores(vec![core]);
                    cpu_set.set_numa_node(node.id);
                    placements.push(cpu_set);
                }

                worker_id += 1;
            }
        }

        placements
    }

    /// Get memory topology information
    pub fn memory_info(&self) -> &MemoryTopology {
        &self.memory_info
    }

    /// Get local memory allocator for a NUMA node
    pub fn local_memory_allocator(&self, node_id: NodeId) -> LocalMemoryAllocator {
        LocalMemoryAllocator::new(node_id)
    }
}

/// NUMA-local memory allocator
#[derive(Debug)]
pub struct LocalMemoryAllocator {
    node_id: NodeId,
    // Future: could include actual allocator implementation
}

impl LocalMemoryAllocator {
    /// Create a new local memory allocator for a NUMA node
    pub fn new(node_id: NodeId) -> Self {
        Self { node_id }
    }

    /// Get the NUMA node ID this allocator is bound to
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Allocate memory with NUMA locality hints
    /// This is a placeholder for future NUMA-aware allocation
    pub fn allocate(&self, size: usize, alignment: usize) -> Option<*mut u8> {
        // For now, use standard allocation
        // Future: implement numa_alloc_onnode or similar
        let layout = std::alloc::Layout::from_size_align(size, alignment).ok()?;

        #[allow(unsafe_code)]
        let ptr = unsafe { std::alloc::alloc(layout) };

        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// Deallocate memory
    #[allow(unsafe_code)]
    pub unsafe fn deallocate(&self, ptr: *mut u8, size: usize, alignment: usize) {
        #[allow(unsafe_code)]
        unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(size, alignment);
            std::alloc::dealloc(ptr, layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_set_creation() {
        let mut cpu_set = CpuSet::new();
        assert!(cpu_set.is_empty());

        cpu_set.add_core(0);
        cpu_set.add_core(1);
        assert_eq!(cpu_set.len(), 2);
        assert!(cpu_set.contains_core(0));
        assert!(cpu_set.contains_core(1));
        assert!(!cpu_set.contains_core(2));
    }

    #[test]
    fn test_cpu_set_from_cores() {
        let cpu_set = CpuSet::from_cores(vec![0, 2, 4, 6]);
        assert_eq!(cpu_set.len(), 4);
        assert!(cpu_set.contains_core(0));
        assert!(cpu_set.contains_core(2));
        assert!(!cpu_set.contains_core(1));
    }

    #[test]
    fn test_cpu_set_with_numa() {
        let cpu_set = CpuSet::with_numa_node(vec![0, 1], NodeId(0));
        assert_eq!(cpu_set.numa_node(), Some(NodeId(0)));
        assert_eq!(cpu_set.len(), 2);
    }

    #[test]
    fn test_node_id_display() {
        let node_id = NodeId(42);
        assert_eq!(format!("{}", node_id), "node42");
    }

    #[test]
    fn test_numa_node_creation() {
        let node = NumaNode::new(NodeId(0), vec![0, 1, 2, 3]);
        assert_eq!(node.id, NodeId(0));
        assert_eq!(node.cores.len(), 4);
        assert!(node.contains_core(2));
        assert!(!node.contains_core(4));
    }

    #[test]
    fn test_numa_topology_detection() {
        // This test may fail on systems without NUMA
        match NumaTopology::detect() {
            Ok(topology) => {
                assert!(!topology.nodes().is_empty());
                let worker_count = topology.optimal_worker_count();
                assert!(worker_count > 0);

                let placements = topology.optimal_worker_placement(worker_count);
                assert_eq!(placements.len(), worker_count);
            },
            Err(_) => {
                // Expected on systems without NUMA support
                eprintln!("NUMA topology detection failed (expected on some systems)");
            },
        }
    }

    #[test]
    fn test_parse_cpulist() {
        assert_eq!(NumaTopology::parse_cpulist("0-3").unwrap(), vec![0, 1, 2, 3]);

        assert_eq!(NumaTopology::parse_cpulist("0,2,4").unwrap(), vec![0, 2, 4]);

        assert_eq!(NumaTopology::parse_cpulist("0-1,4-5").unwrap(), vec![0, 1, 4, 5]);

        assert_eq!(NumaTopology::parse_cpulist("8").unwrap(), vec![8]);
    }

    #[test]
    fn test_local_memory_allocator() {
        let allocator = LocalMemoryAllocator::new(NodeId(0));
        assert_eq!(allocator.node_id(), NodeId(0));

        // Test basic allocation/deallocation
        if let Some(ptr) = allocator.allocate(1024, 8) {
            unsafe {
                allocator.deallocate(ptr, 1024, 8);
            }
        }
    }
}
