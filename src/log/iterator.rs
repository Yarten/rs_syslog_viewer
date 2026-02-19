use crate::log::LogLine;

#[macro_export]
macro_rules! iterator_base {
    ($name:ident, $index_type:ty, $data_type:ty, $get_func:ident $(, $mut_flag:tt)?) => {
      pub struct $name<'a> {
        index: Option<$index_type>,
        data: &'a $($mut_flag)? $data_type,
      }

      impl<'a> $name<'a> {
        fn do_next_nth(&mut self, n: isize, one: isize) -> Result<($index_type, &'a $($mut_flag)? LogLine), usize> {
          match self.index {
            // 当前索引就已经无效，则返回失败
            None => Err(n as usize),

            Some(mut index) => {
              // 正向移动索引以给定步长，若超出了本日志容器的范围，返回剩余步长作为错误信息
              index = self.data.step_index(index, n).map_err(|m| m.abs() as usize)?;

              // 没有报错，则正向迭代一个步长，确保下一次取值从新的下一个元素开始
              self.index = self.data.step_index(index, one).ok();

              // 强制取值，因为 index 一定指向了有效数据
              Ok((index, self.data.$get_func(index).unwrap()))
            }
          }
        }
      }

      impl<'a> Iterator for $name<'a> {
        type Item = ($index_type, &'a $($mut_flag)? LogLine);

        fn next(&mut self) -> Option<Self::Item> {
          self.next_nth(0).ok()
        }
      }
    };
}

#[macro_export]
macro_rules! iterator_func {
    ($name:ident, $index_type:ty, $data_type:ty, $ref_func:ident, $from_func:ident, $from_end_func:ident, $index_func:ident $(, $mut_flag:tt)?) => {
      impl $data_type {
        /// 获取从指定索引位置开始遍历的迭代器
        pub fn $from_func<'a>(& $($mut_flag)? self, index: $index_type) -> $name<'a> {
          $name {
            index: self.step_index(index, 0).ok(),
            data: self.$ref_func(),
          }
        }

        /// 获取从某一端开始遍历的迭代器
        pub fn $from_end_func<'a>(& $($mut_flag)? self) -> $name<'a> {
          self.$from_func(self.$index_func())
        }
      }
    };
}

#[macro_export]
macro_rules! forward_iterator_func {
  ($name:ident, $index_type:ty, $data_type:ty) => {
    crate::iterator_func!(
      $name,
      $index_type,
      $data_type,
      unsafe_ref,
      iter_forward_from,
      iter_forward_from_head,
      first_index
    );
  };
  ($name:ident, $index_type:ty, $data_type:ty, mut) => {
    crate::iterator_func!(
      $name,
      $index_type,
      $data_type,
      unsafe_mut_ref,
      iter_mut_forward_from,
      iter_mut_forward_from_head,
      first_index,
      mut
    );
  };
}

#[macro_export]
macro_rules! backward_iterator_func {
  ($name:ident, $index_type:ty, $data_type:ty) => {
    crate::iterator_func!(
      $name,
      $index_type,
      $data_type,
      unsafe_ref,
      iter_backward_from,
      iter_backward_from_tail,
      last_index
    );
  };
  ($name:ident, $index_type:ty, $data_type:ty, mut) => {
    crate::iterator_func!(
      $name,
      $index_type,
      $data_type,
      unsafe_mut_ref,
      iter_mut_backward_from,
      iter_mut_backward_from_tail,
      last_index,
      mut
    );
  };
}

#[macro_export]
macro_rules! forward_iterator {
    ($name:ident, $index_type:ty, $data_type:ty, $get_func:ident $(, $mut_flag:tt)?) => {
      crate::iterator_base!($name, $index_type, $data_type, $get_func $(, $mut_flag)?);
      crate::forward_iterator_func!($name, $index_type, $data_type  $(, $mut_flag)?);

      impl<'a> $name<'a> {
        pub fn next_nth(&mut self, n: usize) -> Result<($index_type, &'a $($mut_flag)? LogLine), usize> {
          self.do_next_nth(n as isize, 1)
        }
      }
    };
}

#[macro_export]
macro_rules! backward_iterator {
    ($name:ident, $index_type:ty, $data_type:ty, $get_func:ident $(, $mut_flag:tt)?) => {
      crate::iterator_base!($name, $index_type, $data_type, $get_func $(, $mut_flag)?);
      crate::backward_iterator_func!($name, $index_type, $data_type  $(, $mut_flag)?);

      impl<'a> $name<'a> {
        pub fn next_nth(&mut self, n: usize) -> Result<($index_type, &'a $($mut_flag)? LogLine), usize> {
          self.do_next_nth(-(n as isize), -1)
        }
      }
    };
}

#[macro_export]
macro_rules! define_all_iterators {
  ($data_type:ty, $index_type:ty) => {
    crate::forward_iterator!(ForwardIter, $index_type, $data_type, get);
    crate::forward_iterator!(ForwardIterMut, $index_type, $data_type, get_mut, mut);
    crate::backward_iterator!(BackwardIter, $index_type, $data_type, get);
    crate::backward_iterator!(BackwardIterMut, $index_type, $data_type, get_mut, mut);

    impl $data_type {
      fn unsafe_ref<'a>(&self) -> &'a Self {
        unsafe { &*(self as *const $data_type) }
      }

      fn unsafe_mut_ref<'a>(&mut self) -> &'a mut Self {
        unsafe { &mut *(self as *mut $data_type) }
      }
    }
  };
}
