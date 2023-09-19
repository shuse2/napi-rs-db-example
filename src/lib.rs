#![deny(clippy::all)]

use std::cmp::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

use napi::bindgen_prelude::*;
use napi::{
  threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode, ThreadSafeCallContext},
  Task,
};

#[macro_use]
extern crate napi_derive;

#[napi]
pub fn sum(a: i32, b: i32) -> i32 {
  a + b
}

struct WaitGroup {
  counter: Mutex<usize>,
}

impl WaitGroup {
  fn new() -> Self {
      WaitGroup {
          counter: Mutex::new(0),
      }
  }

  fn add(&self, delta: usize) {
      let mut count = self.counter.lock().unwrap();
      *count += delta;
  }

  fn done(&self) {
      let mut count = self.counter.lock().unwrap();
      if *count > 0 {
          *count -= 1;
      }
  }

  fn wait(&self) {
      loop {
          let count = *self.counter.lock().unwrap();
          if count == 0 {
              break;
          }
          // Sleep or yield to avoid busy-waiting
          thread::yield_now();
      }
  }
}

#[napi(object)]
pub struct DBOptions {
  pub readonly: bool,
}

#[napi]
pub struct Database {
  db: Arc<Mutex<rocksdb::DB>>,
}

fn map_rocksdb_err(e: rocksdb::Error) -> Error {
  Error::new(napi::Status::GenericFailure, e.to_string())
}

#[napi]
pub fn create_db(path: String, opts: Option<DBOptions>) -> Result<Database> {
  let mut option = rocksdb::Options::default();
  option.create_if_missing(true);

  let db: rocksdb::DB = if opts.unwrap_or(DBOptions { readonly: false }).readonly {
    rocksdb::DB::open_for_read_only(&option, path, false).map_err(map_rocksdb_err)?
  } else {
    rocksdb::DB::open(&option, path).map_err(map_rocksdb_err)?
  };

  Ok(Database {
    db: Arc::new(Mutex::new(db)),
  })
}

#[napi]
pub fn get(db: &Database, key: Buffer) -> AsyncTask<AsyncGet> {
  AsyncTask::new(AsyncGet {
    db: Arc::clone(&db.db),
    key,
  })
}

#[napi(object)]
pub struct IterationOption {
  pub limit: i64,
  pub reverse: bool,
  pub gte: Option<Buffer>,
  pub lte: Option<Buffer>,
}

pub fn get_iteration_mode<'a>(
  options: &IterationOption,
  range: &'a mut Vec<u8>,
) -> rocksdb::IteratorMode<'a> {
  let no_range = options.gte.is_none() && options.lte.is_none();
  if no_range {
    if options.reverse {
      rocksdb::IteratorMode::End
    } else {
      rocksdb::IteratorMode::Start
    }
  } else if options.reverse {
    let lte = options
      .lte
      .clone()
      .unwrap_or_else(|| Buffer::from(vec![255; options.gte.as_ref().unwrap().len()]));
    *range = lte.into();
    rocksdb::IteratorMode::From(range, rocksdb::Direction::Reverse)
  } else {
    let gte = options
      .gte
      .clone()
      .unwrap_or_else(|| Buffer::from(vec![0; options.lte.as_ref().unwrap().len()]));
    *range = gte.into();
    rocksdb::IteratorMode::From(range, rocksdb::Direction::Forward)
  }
}

fn compare(a: &[u8], b: &[u8]) -> Ordering {
  for (ai, bi) in a.iter().zip(b.iter()) {
    match ai.cmp(bi) {
      Ordering::Equal => continue,
      ord => return ord,
    }
  }
  /* if every single element was equal, compare length */
  a.len().cmp(&b.len())
}

fn is_key_out_of_range(options: &IterationOption, key: &[u8], counter: i64) -> bool {
  if options.limit != -1 && counter >= options.limit {
    return true;
  }
  if options.reverse {
    if let Some(gte) = &options.gte {
      let cmp = gte.to_vec();
      if compare(key, &cmp) == Ordering::Less {
        return true;
      }
    }
  } else if let Some(lte) = &options.lte {
    let cmp = lte.to_vec();
    if compare(key, &cmp) == Ordering::Greater {
      return true;
    }
  }

  false
}

#[napi(ts_args_type = "callback: (err: null | Error, result: { key: Buffer; value: Buffer}) => void")]
pub fn get_iterator(
  db: &Database,
  opts: IterationOption,
  callback: JsFunction,
) -> Result<AsyncTask<AsyncIter>> {
  let wg = Arc::new(WaitGroup::new());
  let c_wg = Arc::clone(&wg);
  let tsfn: ThreadsafeFunction<KV, ErrorStrategy::CalleeHandled> = callback
    .create_threadsafe_function(0, move |ctx: ThreadSafeCallContext<KV>| -> std::result::Result<Vec<KV>, napi::Error> {
      let k = ctx.value.key.to_owned();
      let v = ctx.value.value.to_owned();
      c_wg.done();
      napi::Result::Ok(vec![ KV{ key: k, value: v}])
    })
    .map_err(|e| Error::from_reason(e.to_string()))?;

  Ok(AsyncTask::new(AsyncIter {
    options: opts,
    db: Arc::clone(&db.db),
    cb: tsfn.clone(),
    wg: wg,
  }))
}

pub struct AsyncIter {
  options: IterationOption,
  db: Arc<Mutex<rocksdb::DB>>,
  cb: ThreadsafeFunction<KV, ErrorStrategy::CalleeHandled>,
  wg: Arc<WaitGroup>,
}

#[napi(object)]
pub struct KV {
  pub key: Buffer,
  pub value: Buffer,
}

impl Task for AsyncIter {
  type JsValue = u32;
  type Output = u32;

  fn compute(&mut self) -> Result<Self::Output> {
    let ldb = self
      .db
      .lock()
      .map_err(|e| Error::from_reason(e.to_string()))?;
    let iter = ldb.iterator(get_iteration_mode(&self.options, &mut vec![]));
    
    for (counter, key_val) in iter.enumerate() {
      if is_key_out_of_range(
        &self.options,
        &(key_val.as_ref().unwrap().0),
        counter as i64,
      ) {
        break;
      }
      self.wg.add(1);
      let result = key_val.map_err(|e| Error::from_reason(e.to_string()))?;
      let kv = KV {
        key: Buffer::from(&*result.0),
        value: Buffer::from(&*result.1),
      };
      let _result = self.cb.call(Ok(kv), ThreadsafeFunctionCallMode::Blocking);
    }
    self.wg.wait();
    Ok(0)
  }
  fn resolve(&mut self, _env: Env, _output: Self::Output) -> Result<Self::JsValue> {
    Ok(0)
  }

  fn finally(&mut self, env: Env) -> Result<()> {
    self.cb.unref(&env)?;
      println!("done");
    Ok(())
  }
}

pub struct AsyncGet {
  db: Arc<Mutex<rocksdb::DB>>,
  key: Buffer,
}

impl Task for AsyncGet {
  type JsValue = Vec<u8>;
  type Output = Vec<u8>;

  fn compute(&mut self) -> Result<Self::Output> {
    let ldb = self
      .db
      .lock()
      .map_err(|e| Error::from_reason(e.to_string()))?;
    let val = ldb
      .get(&self.key)
      .map_err(map_rocksdb_err)
      .map_err(|e| Error::from_reason(e.to_string()))?;
    let x = val.ok_or(Error::from_reason("not found"))?;
    Ok(x)
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output.into())
  }
}
