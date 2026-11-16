# frozen_string_literal: true

require 'minitest/autorun'
require 'open3'
require 'rbconfig'

# The class of the value the extension wraps (Saga) is fixed when it loads.
# Nothing a caller does later, redefining the constant or
# `Object.const_get`, reaches a saga being built. Each case runs in a fresh
# process: the class is fixed once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  # The values are built before any case touches a constant or `const_get`:
  # their own checks name `Saga`.
  PRELUDE = <<~RUBY
    require 'saga_machines'
    S = SagaMachines
    ID = S::SagaId.new(7)
    SAVED = S::Snapshot.new(ID, 0, S::Forward.new(0), 0, nil)
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Saga constant redefined before the first saga made `new`,
  # `with_scope` and `restored` answer an instance of the new class, with none
  # of the saga's methods.
  def test_a_redefined_saga_constant_does_not_change_the_saga_class
    assert_equal 'true true true 2 :cancelled', isolated(<<~RUBY)
      original = S::Saga
      S.send(:remove_const, :Saga)
      S.const_set(:Saga, Class.new)
      built = [original.new(2, ID), original.with_scope(2, ID, 3), original.restored(2, SAVED, 0)]
      puts [*built.map { it.instance_of?(original) }, built.first.steps, built.last.cancel.inspect].join(' ')
    RUBY
  end

  # Bug: building the first saga ran `Object.const_get`, a caller's code in
  # the middle of a native call. No Ruby runs while a saga is built now.
  def test_building_a_saga_runs_no_const_get
    assert_equal '0 Saga Saga Saga', isolated(<<~RUBY)
      hits = 0
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        super(*args)
      end
      built = [S::Saga.new(2, ID), S::Saga.with_scope(2, ID, 3), S::Saga.restored(2, SAVED, 0)]
      puts [hits, *built.map { it.class.name.split('::').last }].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal '4 0', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran mid-call' }
      saga = S::Saga.new(4, ID)
      puts [saga.steps, saga.issued].join(' ')
    RUBY
  end
end
