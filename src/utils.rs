// align should be a power of 2
pub fn align_to(n: usize, align: usize) -> usize {
	(n + align - 1) & !(align - 1)
}
